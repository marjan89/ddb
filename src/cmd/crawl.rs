use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::adb;
use crate::agent_yaml::{self, chunk_bounds, chunk_top_field};
use crate::cmd::test::wait_idle;
use crate::cmd::test_element::{agent_base_url, curl_get, curl_post};
use crate::registry::{Device, Registry};

#[derive(clap::Args)]
pub struct CrawlArgs {
    #[arg(long, env = "DDB_TEST_PACKAGE")]
    pub package: String,
    #[arg(long, default_value = "catalogue/crawl")]
    pub output: String,
    #[arg(long)]
    pub resume: bool,
    #[arg(long, default_value = "50")]
    pub max_screens: usize,
    #[arg(long, default_value = "10")]
    pub max_scroll_depth: usize,
    #[arg(long, default_value = "Share,Delete,Log out,Uninstall")]
    pub exclude: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct ScreenSnapshot {
    screen_id: String,
    activity: String,
    elements: Vec<CrawlElement>,
    scroll_depth: usize,
    tapped: Vec<String>,
    screenshot: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct CrawlElement {
    content: String,
    #[serde(rename = "type")]
    element_type: String,
    id: Option<String>,
    bounds: Option<[i32; 4]>,
    clickable: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct NavEdge { from: String, element: String, to: String }

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Quirk { screen: String, quirk_type: String, description: String }

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct CrawlState {
    visited: HashMap<String, ScreenSnapshot>,
    edges: Vec<NavEdge>,
    quirks: Vec<Quirk>,
}

// ---------------------------------------------------------------------------
// Field extraction from /semantic chunks (parser shared via agent_yaml::split_elements)
// ---------------------------------------------------------------------------

fn parse_semantic_elements(yaml: &str) -> Vec<CrawlElement> {
    let chunks = agent_yaml::split_elements(yaml);
    let mut out = Vec::new();
    for chunk in &chunks {
        let id = chunk_top_field(chunk, "platform_id")
            .or_else(|| chunk_top_field(chunk, "id"));
        let content = chunk_top_field(chunk, "content").unwrap_or_default();
        let etype = chunk_top_field(chunk, "type").unwrap_or_default();
        let clickable = chunk_top_field(chunk, "clickable")
            .map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false);
        let bounds = chunk_bounds(chunk);
        if id.is_none() && content.is_empty() && etype.is_empty() && bounds.is_none() && !clickable {
            continue;
        }
        out.push(CrawlElement { content, element_type: etype, id, bounds, clickable });
    }
    out
}

fn element_dedup_key(e: &CrawlElement) -> String {
    if let Some(ref id) = e.id { return format!("id:{id}"); }
    if !e.content.is_empty() { return format!("content:{}", e.content); }
    if let Some(b) = e.bounds { return format!("bounds:{},{},{},{}", b[0], b[1], b[2], b[3]); }
    format!("type:{}:click:{}", e.element_type, e.clickable)
}

// ---------------------------------------------------------------------------
// Device-side primitives — all delegated to crate::adb
// ---------------------------------------------------------------------------

fn get_activity(dev: Option<&Device>) -> String {
    // Single source of truth: mResumedActivity / topResumedActivity (Samsung).
    adb::shell(dev, &["dumpsys", "activity", "activities"])
        .ok()
        .and_then(|out| {
            let line = out.lines().find(|l| {
                let t = l.trim_start();
                t.starts_with("mResumedActivity") || t.starts_with("topResumedActivity")
            })?;
            let bracket = line.split('{').nth(1)?;
            let inside = bracket.split('}').next()?;
            inside.split_whitespace().find(|t| t.contains('/')).map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

fn current_foreground_pkg(dev: Option<&Device>) -> Option<String> {
    let out = adb::shell(dev, &["dumpsys", "activity", "activities"]).ok()?;
    let line = out.lines().find(|l| {
        let t = l.trim_start();
        t.starts_with("mResumedActivity") || t.starts_with("topResumedActivity")
    })?;
    let bracket = line.split('{').nth(1)?;
    let inside = bracket.split('}').next()?;
    let token = inside.split_whitespace().find(|t| t.contains('/'))?;
    Some(token.split('/').next()?.to_string())
}

fn is_app_alive(dev: Option<&Device>, pkg: &str) -> bool {
    adb::shell(dev, &["pidof", pkg]).map(|s| !s.trim().is_empty()).unwrap_or(false)
}

fn is_launcher_pkg(pkg: &str) -> bool {
    // Match common launcher package suffixes:
    //   *.launcher           (vendor + AOSP, e.g. com.sec.android.app.launcher)
    //   *.launcher<digits>   (versioned AOSP, e.g. com.android.launcher3)
    //   *nexuslauncher       (Pixel launcher)
    if pkg.contains("nexuslauncher") { return true; }
    let trimmed = pkg.trim_end_matches(|c: char| c.is_ascii_digit());
    trimmed.ends_with(".launcher")
}

#[cfg(test)]
mod is_launcher_tests {
    use super::is_launcher_pkg;
    #[test] fn matches_samsung() { assert!(is_launcher_pkg("com.sec.android.app.launcher")); }
    #[test] fn matches_aosp_versioned() { assert!(is_launcher_pkg("com.android.launcher3")); }
    #[test] fn matches_pixel() { assert!(is_launcher_pkg("com.google.android.apps.nexuslauncher")); }
    #[test] fn rejects_third_party() {
        assert!(!is_launcher_pkg("com.example.someapp"));
        assert!(!is_launcher_pkg("com.example.target"));
        assert!(!is_launcher_pkg("com.google.android.apps.maps"));
    }
}

fn forward_agent_port(dev: Option<&Device>) {
    let port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    let _ = adb::adb(dev, &["forward", &format!("tcp:{port}"), "tcp:9876"]);
}

fn wait_for_foreground(dev: Option<&Device>, target: &str, timeout_ms: u64) -> bool {
    let step_ms = 200u64;
    let mut elapsed = 0u64;
    while elapsed < timeout_ms {
        if let Some(fg) = current_foreground_pkg(dev) {
            if fg == target { return true; }
        }
        std::thread::sleep(std::time::Duration::from_millis(step_ms));
        elapsed += step_ms;
    }
    false
}

fn take_screenshot(dev: Option<&Device>, path: &str) -> bool {
    let remote = "/sdcard/crawl_screenshot.png";
    let _ = adb::shell(dev, &["screencap", "-p", remote]);
    adb::adb(dev, &["pull", remote, path]).is_ok()
}

fn launch_app(dev: Option<&Device>, main_activity: &str, pkg: &str) {
    // -W blocks until activity is fully started + visible.
    let _ = adb::shell(dev, &[
        "am", "start", "-W",
        "-a", "android.intent.action.MAIN",
        "-c", "android.intent.category.LAUNCHER",
        "-n", main_activity,
    ]);
    let _ = wait_for_foreground(dev, pkg, 5_000);
    forward_agent_port(dev);
}

// ---------------------------------------------------------------------------
// Fingerprint
// ---------------------------------------------------------------------------

fn fingerprint(activity: &str, elements: &[CrawlElement]) -> String {
    let mut set: Vec<String> = elements.iter()
        .map(|e| format!("{}:{}", e.id.as_deref().unwrap_or(&e.content), e.element_type))
        .collect();
    set.push(format!("activity:{activity}"));
    set.sort();
    let hash = set.join("|");
    format!("{:x}", fnv_hash(&hash))
}

fn fnv_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}

// ---------------------------------------------------------------------------
// Screen capture (settle via shared wait_idle, then read /semantic)
// ---------------------------------------------------------------------------

fn settle_timeout_s() -> u64 {
    std::env::var("DDB_SETTLE_S").ok().and_then(|s| s.parse().ok()).unwrap_or(15)
}

fn capture_elements(dev: Option<&Device>, base: &str) -> Vec<CrawlElement> {
    wait_idle(dev, settle_timeout_s());
    match curl_get(&format!("{base}/semantic")) {
        Ok(sem) => parse_semantic_elements(&sem),
        Err(_) => Vec::new(),
    }
}

fn scroll_and_discover(dev: Option<&Device>, base: &str, max_depth: usize) -> Vec<CrawlElement> {
    let mut all_elements = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..max_depth {
        let elems = capture_elements(dev, base);
        let mut new_count = 0;
        for e in &elems {
            if seen.insert(element_dedup_key(e)) {
                all_elements.push(e.clone());
                new_count += 1;
            }
        }
        if new_count == 0 { break; }
        let _ = adb::shell(dev, &["input", "swipe", "540", "1500", "540", "500", "300"]);
    }
    all_elements
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

pub fn run(dev_name: Option<&str>, args: CrawlArgs) -> Result<(), String> {
    let out_dir = PathBuf::from(&args.output);
    std::fs::create_dir_all(out_dir.join("screens")).map_err(|e| format!("mkdir: {e}"))?;
    std::fs::create_dir_all(out_dir.join("screenshots")).map_err(|e| format!("mkdir: {e}"))?;

    let state_path = out_dir.join("crawl-state.yaml");
    let mut state: CrawlState = if args.resume && state_path.exists() {
        serde_yaml::from_str(&std::fs::read_to_string(&state_path).unwrap_or_default()).unwrap_or_default()
    } else { CrawlState::default() };

    let exclude: Vec<String> = args.exclude.split(',').map(|s| s.trim().to_lowercase()).collect();
    let base = agent_base_url();
    let mut visit_counts: HashMap<String, usize> = HashMap::new();

    eprintln!("Crawling {} (max {} screens)", args.package, args.max_screens);

    // Resolve device once; pass dev.as_ref() everywhere.
    let devices = Registry::load()?;
    let dev: Option<Device> = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    // Setup: DDB_CLEAN_STATE branches pm clear+grant vs force-stop.
    let clean_state = std::env::var("DDB_CLEAN_STATE").ok().map(|v| v == "true").unwrap_or(false);
    if clean_state {
        let _ = adb::shell(dev.as_ref(), &["pm", "clear", &args.package]);
        let perms = format!(
            "pm grant {pkg} android.permission.ACCESS_FINE_LOCATION; \
             pm grant {pkg} android.permission.ACCESS_COARSE_LOCATION; \
             pm grant {pkg} android.permission.POST_NOTIFICATIONS",
            pkg = args.package
        );
        let _ = adb::shell(dev.as_ref(), &[&perms]);
    } else {
        let _ = adb::shell(dev.as_ref(), &["am", "force-stop", &args.package]);
    }

    std::thread::sleep(std::time::Duration::from_millis(500));
    let main_activity = std::env::var("DDB_MAIN_ACTIVITY")
        .unwrap_or_else(|_| format!("{}/.MainActivity", args.package));
    launch_app(dev.as_ref(), &main_activity, &args.package);

    // Health check — fail fast if agent never comes up.
    let mut ready = false;
    for _ in 0..10 {
        if curl_get(&format!("{base}/health")).map(|b| b.contains("semantic-agent")).unwrap_or(false) {
            ready = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !ready { return Err("agent not ready after 5s".into()); }

    let mut screens_to_explore: Vec<String> = vec!["initial".into()];

    while let Some(current_screen) = screens_to_explore.pop() {
        if state.visited.len() >= args.max_screens { break; }

        // Crash detection — re-queue + retry.
        if !is_app_alive(dev.as_ref(), &args.package) {
            eprintln!("  CRASH detected — relaunching");
            state.quirks.push(Quirk {
                screen: "unknown".into(),
                quirk_type: "crash".into(),
                description: "app crashed during crawl".into(),
            });
            launch_app(dev.as_ref(), &main_activity, &args.package);
            screens_to_explore.push(current_screen);
            continue;
        }

        // Foreground guard — re-queue and retry; never force-stop a launcher.
        if let Some(fg) = current_foreground_pkg(dev.as_ref()) {
            if fg != args.package {
                let is_launcher = is_launcher_pkg(&fg);
                eprintln!("  FG MISMATCH: {} (launcher={}) — relaunching {}", fg, is_launcher, args.package);
                if !is_launcher {
                    state.quirks.push(Quirk {
                        screen: "unknown".into(),
                        quirk_type: "foreground_intrusion".into(),
                        description: format!("{} took foreground during crawl", fg),
                    });
                    let _ = adb::shell(dev.as_ref(), &["am", "force-stop", &fg]);
                }
                launch_app(dev.as_ref(), &main_activity, &args.package);
                screens_to_explore.push(current_screen);
                continue;
            }
        }
        let _ = current_screen;

        // Discover elements (settles via shared wait_idle, scrolls for off-screen content).
        let all_elements = if args.max_scroll_depth > 0 {
            scroll_and_discover(dev.as_ref(), &base, args.max_scroll_depth)
        } else {
            capture_elements(dev.as_ref(), &base)
        };

        if all_elements.is_empty() {
            eprintln!("  /semantic returned 0 elements — skipping iteration");
            continue;
        }

        let activity = get_activity(dev.as_ref());

        if std::env::var("DDB_CRAWL_DEBUG").ok().as_deref() == Some("1") {
            for (i, e) in all_elements.iter().enumerate() {
                eprintln!("  [DBG {:03}] click={} id={:?} type={:?} content={:?} bounds={:?}",
                    i, e.clickable, e.id, e.element_type, e.content, e.bounds);
            }
        }

        let screen_id = fingerprint(&activity, &all_elements);
        let count = visit_counts.entry(screen_id.clone()).or_insert(0);
        *count += 1;
        if *count > 3 { continue; }

        if all_elements.iter().any(|e| e.element_type.contains("WebView")) {
            state.quirks.push(Quirk {
                screen: screen_id.clone(),
                quirk_type: "webview".into(),
                description: "screen contains WebView — opaque to crawler".into(),
            });
        }

        if !state.visited.contains_key(&screen_id) {
            eprintln!("  NEW: {} [{}] ({} elements)", screen_id, activity, all_elements.len());
            let ss_path = out_dir.join("screenshots").join(format!("{screen_id}.png"));
            let ss_name = if take_screenshot(dev.as_ref(), ss_path.to_str().unwrap_or("")) {
                Some(format!("screenshots/{screen_id}.png"))
            } else { None };
            let snapshot = ScreenSnapshot {
                screen_id: screen_id.clone(),
                activity: activity.clone(),
                elements: all_elements.clone(),
                scroll_depth: args.max_scroll_depth,
                tapped: Vec::new(),
                screenshot: ss_name,
            };
            let _ = std::fs::write(
                out_dir.join("screens").join(format!("{screen_id}.yaml")),
                serde_yaml::to_string(&snapshot).unwrap_or_default(),
            );
            state.visited.insert(screen_id.clone(), snapshot);
        }

        // Tappable = clickable + addressable (id | content | bounds), not excluded, not already tapped.
        let total = all_elements.len();
        let after_clickable: Vec<&CrawlElement> = all_elements.iter()
            .filter(|e| e.clickable && (e.id.is_some() || !e.content.is_empty() || e.bounds.is_some()))
            .collect();
        let after_exclude: Vec<&CrawlElement> = after_clickable.iter().copied()
            .filter(|e| !exclude.iter().any(|p| e.content.to_lowercase().contains(p)))
            .collect();
        let tappable: Vec<&CrawlElement> = after_exclude.iter().copied()
            .filter(|e| {
                let key = element_dedup_key(e);
                state.visited.get(&screen_id).map_or(true, |s| !s.tapped.contains(&key))
            })
            .collect();

        eprintln!("  TAPPABLE: total={} clickable+addr={} after-exclude={} after-dedup={}",
            total, after_clickable.len(), after_exclude.len(), tappable.len());

        if tappable.is_empty() {
            eprintln!("  SKIP: no tappable on screen {} (visit {}/3)", screen_id, count);
            continue;
        }

        let elem = tappable[0];
        let label = if !elem.content.is_empty() {
            elem.content.clone()
        } else if let Some(ref id) = elem.id {
            id.clone()
        } else if let Some(b) = elem.bounds {
            format!("[{},{},{},{}]", b[0], b[1], b[2], b[3])
        } else {
            "<unknown>".into()
        };
        eprintln!("  TAP: '{}'", label);

        let key = element_dedup_key(elem);
        if let Some(s) = state.visited.get_mut(&screen_id) { s.tapped.push(key); }

        // Tap precedence: resource_id (id) > content_fuzzy > center-of-bounds (raw input tap).
        let click_body = if let Some(ref id) = elem.id {
            Some(serde_json::json!({"resource_id": id}).to_string())
        } else if !elem.content.is_empty() {
            Some(serde_json::json!({"content_fuzzy": elem.content}).to_string())
        } else if let Some(b) = elem.bounds {
            let cx = (b[0] + b[2]) / 2;
            let cy = (b[1] + b[3]) / 2;
            let _ = adb::shell(dev.as_ref(), &["input", "tap", &cx.to_string(), &cy.to_string()]);
            None
        } else {
            None
        };
        if let Some(body) = click_body {
            let _ = curl_post(&format!("{base}/click"), &body);
        }

        // Wait for the app to settle after the tap before sampling the new screen.
        wait_idle(dev.as_ref(), settle_timeout_s());

        // Crash check after tap.
        if !is_app_alive(dev.as_ref(), &args.package) {
            eprintln!("  CRASH after tapping '{}'", label);
            state.quirks.push(Quirk {
                screen: screen_id.clone(),
                quirk_type: "crash".into(),
                description: format!("crash after tapping '{}'", label),
            });
            launch_app(dev.as_ref(), &main_activity, &args.package);
            screens_to_explore.push(screen_id);
            continue;
        }

        // Did this tap move us to a new screen?
        let new_activity = get_activity(dev.as_ref());
        if let Ok(new_sem) = curl_get(&format!("{base}/semantic")) {
            let new_elems = parse_semantic_elements(&new_sem);
            let new_id = fingerprint(&new_activity, &new_elems);
            let debug = std::env::var("DDB_CRAWL_DEBUG").ok().as_deref() == Some("1");
            if debug {
                eprintln!("  [TAP-CMP] before={} after={} new_activity={} same={}",
                    screen_id, new_id, new_activity, new_id == screen_id);
            }
            if new_id != screen_id {
                eprintln!("  EDGE: '{}' -> {}", label, new_id);
                state.edges.push(NavEdge {
                    from: screen_id.clone(),
                    element: label.clone(),
                    to: new_id.clone(),
                });
                screens_to_explore.push(new_id);
            } else if debug {
                eprintln!("  NO-NAV: tap on '{}' did not change screen fingerprint", label);
            }
        }

        // Navigate back via adb keyevent (a single-shot, no execute_action ceremony needed).
        let _ = adb::shell(dev.as_ref(), &["input", "keyevent", "KEYCODE_BACK"]);
        wait_idle(dev.as_ref(), settle_timeout_s());

        screens_to_explore.push(screen_id);
        let _ = std::fs::write(&state_path, serde_yaml::to_string(&state).unwrap_or_default());
    }

    // Final outputs.
    let _ = std::fs::write(out_dir.join("navigation-graph.yaml"),
        serde_yaml::to_string(&state.edges).unwrap_or_default());
    let _ = std::fs::write(out_dir.join("quirks.yaml"),
        serde_yaml::to_string(&state.quirks).unwrap_or_default());
    let _ = std::fs::write(&state_path, serde_yaml::to_string(&state).unwrap_or_default());

    eprintln!("Crawl done: {} screens, {} edges, {} quirks",
        state.visited.len(), state.edges.len(), state.quirks.len());
    Ok(())
}
