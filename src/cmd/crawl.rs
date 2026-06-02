use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::agent_yaml::{self, ElementRecord};
use crate::registry::Registry;

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

impl From<&ElementRecord> for CrawlElement {
    fn from(r: &ElementRecord) -> Self {
        CrawlElement {
            content: r.content.clone(),
            element_type: r.etype.clone(),
            id: r.id.clone(),
            bounds: r.bounds,
            clickable: r.clickable,
        }
    }
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

fn agent_base_url() -> String {
    let port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    format!("http://127.0.0.1:{port}")
}

fn curl_get(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "10", url])
        .output().map_err(|e| format!("curl: {e}"))?;
    if output.status.success() { Ok(String::from_utf8_lossy(&output.stdout).to_string()) }
    else { Err("curl non-zero".into()) }
}

fn curl_post(url: &str, body: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "10", "-X", "POST", "-d", body, url])
        .output().map_err(|e| format!("curl: {e}"))?;
    if output.status.success() { Ok(String::from_utf8_lossy(&output.stdout).to_string()) }
    else { Err("curl non-zero".into()) }
}

fn adb_shell(dev: Option<&str>, args: &[&str]) -> Result<String, String> {
    let mut cmd = std::process::Command::new("adb");
    if let Some(d) = dev { cmd.args(["-s", d]); }
    cmd.arg("shell").args(args);
    let output = cmd.output().map_err(|e| format!("adb: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn get_activity(dev: Option<&str>) -> String {
    // Single source of truth: mResumedActivity (one resumed activity per device).
    // Returns full "pkg/.ClassName" token to match prior format.
    adb_shell(dev, &["dumpsys", "activity", "activities"])
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

fn is_app_alive(dev: Option<&str>, pkg: &str) -> bool {
    adb_shell(dev, &["pidof", pkg]).map(|s| !s.trim().is_empty()).unwrap_or(false)
}

fn current_foreground_pkg(dev: Option<&str>) -> Option<String> {
    // Samsung uses topResumedActivity, AOSP uses mResumedActivity. Both are singletons.
    let out = adb_shell(dev, &["dumpsys", "activity", "activities"]).ok()?;
    let line = out.lines().find(|l| {
        let t = l.trim_start();
        t.starts_with("mResumedActivity") || t.starts_with("topResumedActivity")
    })?;
    let bracket = line.split('{').nth(1)?;
    let inside = bracket.split('}').next()?;
    let token = inside.split_whitespace().find(|t| t.contains('/'))?;
    let pkg = token.split('/').next()?;
    Some(pkg.to_string())
}

fn is_launcher_pkg(pkg: &str) -> bool {
    pkg.ends_with(".launcher") || pkg.contains("nexuslauncher")
}

fn forward_agent_port(dev: Option<&str>) {
    // adb forward dies after pm clear + relaunch (new app PID + new socket).
    // Cheap to re-establish; call after every launch/relaunch.
    if let Some(s) = dev {
        let agent_port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
        let mut fwd = std::process::Command::new("adb");
        fwd.args(["-s", s]).args(["forward", &format!("tcp:{agent_port}"), "tcp:9876"]);
        let _ = fwd.output();
    }
}

fn wait_for_foreground(dev: Option<&str>, target: &str, timeout_ms: u64) -> bool {
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

fn take_screenshot(dev: Option<&str>, path: &str) -> bool {
    let remote = "/sdcard/crawl_screenshot.png";
    let _ = adb_shell(dev, &["screencap", "-p", remote]);
    let mut cmd = std::process::Command::new("adb");
    if let Some(d) = dev { cmd.args(["-s", d]); }
    cmd.args(["pull", remote, path]).output().map(|o| o.status.success()).unwrap_or(false)
}

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

fn parse_semantic_elements(yaml: &str) -> Vec<CrawlElement> {
    agent_yaml::parse_elements(yaml).iter().map(CrawlElement::from).collect()
}

fn element_dedup_key(e: &CrawlElement) -> String {
    if let Some(ref id) = e.id { return format!("id:{id}"); }
    if !e.content.is_empty() { return format!("content:{}", e.content); }
    if let Some(b) = e.bounds { return format!("bounds:{},{},{},{}", b[0], b[1], b[2], b[3]); }
    format!("type:{}:click:{}", e.element_type, e.clickable)
}

fn scroll_and_discover(dev: Option<&str>, base: &str, max_depth: usize) -> Vec<CrawlElement> {
    let mut all_elements = Vec::new();
    let mut seen_contents: HashSet<String> = HashSet::new();

    for _ in 0..max_depth {
        if let Ok(sem) = curl_get(&format!("{base}/semantic")) {
            let elems = parse_semantic_elements(&sem);
            let mut new_count = 0;
            for e in &elems {
                // Use composite key: id-first, content-second, bounds-third — empty
                // content alone collides for every unlabeled clickable.
                if seen_contents.insert(element_dedup_key(e)) {
                    all_elements.push(e.clone());
                    new_count += 1;
                }
            }
            if new_count == 0 { break; }
        }
        let _ = adb_shell(dev, &["input", "swipe", "540", "1500", "540", "500", "300"]);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    all_elements
}

pub fn run(dev_arg: Option<&str>, args: CrawlArgs) -> Result<(), String> {
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

    // Resolve device name → serial via Registry (adb -s needs serial, not friendly name)
    let devices = Registry::load()?;
    let serial: Option<String> = if devices.is_empty() && dev_arg.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_arg, &devices)?;
        Some(d.transport_id())
    };
    let dev_arg: Option<&str> = serial.as_deref();

    // Setup: matches ddb test precondition flow (DDB_CLEAN_STATE branches pm clear vs force-stop)
    let clean_state = std::env::var("DDB_CLEAN_STATE").ok().map(|v| v == "true").unwrap_or(false);
    if clean_state {
        let _ = adb_shell(dev_arg, &["pm", "clear", &args.package]);
        let perms = format!(
            "pm grant {pkg} android.permission.ACCESS_FINE_LOCATION; pm grant {pkg} android.permission.ACCESS_COARSE_LOCATION; pm grant {pkg} android.permission.POST_NOTIFICATIONS",
            pkg = args.package
        );
        let _ = adb_shell(dev_arg, &[&perms]);
    } else {
        let _ = adb_shell(dev_arg, &["am", "force-stop", &args.package]);
    }

    // Launch app
    std::thread::sleep(std::time::Duration::from_millis(500));
    let main_activity = std::env::var("DDB_MAIN_ACTIVITY").unwrap_or_else(|_| format!("{}/.MainActivity", args.package));
    // -W blocks until activity is fully started; poll mCurrentFocus as belt+suspenders
    let _ = adb_shell(dev_arg, &["am", "start", "-W", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
    if !wait_for_foreground(dev_arg, &args.package, 5_000) {
        eprintln!("  WARN: target {} did not reach foreground within 5s after launch", args.package);
    }

    // Port forwarding for agent (re-establish after every launch — dies on pm clear + relaunch)
    forward_agent_port(dev_arg);

    // Health check: hard fail if agent not ready (10 × 500ms)
    let mut ready = false;
    for _ in 0..10 {
        if curl_get(&format!("{base}/health")).map(|b| b.contains("semantic-agent")).unwrap_or(false) { ready = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !ready { return Err("agent not ready after 5s".into()); }

    // Content settle: after launch the app's chrome appears immediately but
    // dynamic content (lists, network-driven views) renders asynchronously.
    // Poll /semantic until the parsed element count is stable across two
    // consecutive ticks (or timeout). Generic — no app strings, no element
    // names. Tunable via DDB_SETTLE_MS / DDB_SETTLE_TICK_MS.
    let settle_budget_ms: u64 = std::env::var("DDB_SETTLE_MS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(8_000);
    let settle_tick_ms: u64 = std::env::var("DDB_SETTLE_TICK_MS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(500);
    let mut last_count: Option<usize> = None;
    let mut elapsed = 0u64;
    while elapsed < settle_budget_ms {
        if let Ok(sem) = curl_get(&format!("{base}/semantic")) {
            let n = parse_semantic_elements(&sem).len();
            if last_count == Some(n) && n > 0 {
                eprintln!("Content settled at {} elements after {}ms", n, elapsed);
                break;
            }
            last_count = Some(n);
        }
        std::thread::sleep(std::time::Duration::from_millis(settle_tick_ms));
        elapsed += settle_tick_ms;
    }

    let mut screens_to_explore: Vec<String> = vec!["initial".into()];

    while let Some(current_screen) = screens_to_explore.pop() {
        if state.visited.len() >= args.max_screens { break; }

        // Crash detection — re-queue current and retry
        if !is_app_alive(dev_arg, &args.package) {
            eprintln!("  CRASH detected — relaunching");
            state.quirks.push(Quirk { screen: "unknown".into(), quirk_type: "crash".into(), description: "app crashed during crawl".into() });
            let _ = adb_shell(dev_arg, &["am", "start", "-W", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
            let _ = wait_for_foreground(dev_arg, &args.package, 5_000);
            forward_agent_port(dev_arg);
            screens_to_explore.push(current_screen);
            continue;
        }

        // Foreground guard: re-queue current screen and retry. Force-stop only third-party
        // intruders — never force-stop a launcher.
        if let Some(fg) = current_foreground_pkg(dev_arg) {
            if fg != args.package {
                let is_launcher = is_launcher_pkg(&fg);
                eprintln!("  FG MISMATCH: {} (launcher={}) — relaunching {}", fg, is_launcher, args.package);
                if !is_launcher {
                    state.quirks.push(Quirk {
                        screen: "unknown".into(),
                        quirk_type: "foreground_intrusion".into(),
                        description: format!("{} took foreground during crawl", fg),
                    });
                    let _ = adb_shell(dev_arg, &["am", "force-stop", &fg]);
                }
                let _ = adb_shell(dev_arg, &["am", "start", "-W", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
                let _ = wait_for_foreground(dev_arg, &args.package, 5_000);
                forward_agent_port(dev_arg);
                screens_to_explore.push(current_screen);
                continue;
            }
        }
        let _ = current_screen;

        // Get current screen — retry up to 3× with 1s gap if /semantic returns no elements
        // (covers the case where activity is resumed but UI hasn't fully rendered yet).
        let mut semantic = String::new();
        let mut elements: Vec<CrawlElement> = Vec::new();
        let mut semantic_ok = false;
        for attempt in 0..3 {
            match curl_get(&format!("{base}/semantic")) {
                Ok(s) => {
                    let parsed = parse_semantic_elements(&s);
                    if !parsed.is_empty() {
                        semantic = s;
                        elements = parsed;
                        semantic_ok = true;
                        break;
                    }
                    // Diagnostic: surface raw body so we can tell parser-bug from truly-empty
                    let preview: String = s.chars().take(200).collect();
                    eprintln!("  /semantic empty (attempt {}/3) — raw body (len={}, first 200): {:?}",
                        attempt + 1, s.len(), preview);
                    if attempt + 1 < 3 {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
                Err(_) => { eprintln!("  /semantic unreachable"); break; }
            }
        }
        if !semantic_ok && elements.is_empty() {
            eprintln!("  /semantic returned 0 elements after 3 attempts — skipping iteration");
            continue;
        }

        let activity = get_activity(dev_arg);

        // Scroll discovery
        let all_elements = if args.max_scroll_depth > 0 {
            scroll_and_discover(dev_arg, &base, args.max_scroll_depth)
        } else { elements.clone() };

        // Opt-in debug: dump every parsed element on one line. Triggered by
        // DDB_CRAWL_DEBUG=1 so it doesn't pollute normal runs.
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

        // WebView detection
        if all_elements.iter().any(|e| e.element_type.contains("WebView")) {
            state.quirks.push(Quirk { screen: screen_id.clone(), quirk_type: "webview".into(), description: "screen contains WebView — opaque to crawler".into() });
        }

        if !state.visited.contains_key(&screen_id) {
            eprintln!("  NEW: {} [{}] ({} elements)", screen_id, activity, all_elements.len());

            // Screenshot
            let ss_path = out_dir.join("screenshots").join(format!("{screen_id}.png"));
            let ss_name = if take_screenshot(dev_arg, ss_path.to_str().unwrap_or("")) {
                Some(format!("screenshots/{screen_id}.png"))
            } else { None };

            let snapshot = ScreenSnapshot {
                screen_id: screen_id.clone(), activity: activity.clone(),
                elements: all_elements.clone(), scroll_depth: args.max_scroll_depth,
                tapped: Vec::new(), screenshot: ss_name,
            };
            let _ = std::fs::write(out_dir.join("screens").join(format!("{screen_id}.yaml")),
                serde_yaml::to_string(&snapshot).unwrap_or_default());
            state.visited.insert(screen_id.clone(), snapshot);
        }

        // Find untapped clickable elements — must be clickable AND addressable
        // (either content text OR a stable id). Touch targets like 'discoverTouch'
        // have id but no content; bottom-nav text labels have content but no id.
        // Tappable = clickable + addressable. Addressable means we have at least
        // one of: stable id, content text, or bounds (for unlabeled touch zones).
        let total = all_elements.len();
        let after_clickable: Vec<&CrawlElement> = all_elements.iter()
            .filter(|e| e.clickable && (e.id.is_some() || !e.content.is_empty() || e.bounds.is_some()))
            .collect();
        let after_exclude: Vec<&CrawlElement> = after_clickable.iter()
            .copied()
            .filter(|e| !exclude.iter().any(|p| e.content.to_lowercase().contains(p)))
            .collect();
        let tappable: Vec<&CrawlElement> = after_exclude.iter()
            .copied()
            .filter(|e| {
                let key = element_dedup_key(e);
                state.visited.get(&screen_id).map_or(true, |s| !s.tapped.contains(&key))
            })
            .collect();

        eprintln!("  TAPPABLE: total={} clickable+addr={} after-exclude={} after-dedup={}",
            total, after_clickable.len(), after_exclude.len(), tappable.len());

        if tappable.is_empty() {
            // Visit-count cap also forces a continue. Note it for the operator.
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

        // Record as tapped using the same key
        let key = element_dedup_key(elem);
        if let Some(s) = state.visited.get_mut(&screen_id) { s.tapped.push(key); }

        // Execute tap. Precedence: resource_id (id) > content_fuzzy > center-of-bounds.
        let click_body = if let Some(ref id) = elem.id {
            serde_json::json!({"resource_id": id}).to_string()
        } else if !elem.content.is_empty() {
            serde_json::json!({"content_fuzzy": elem.content}).to_string()
        } else if let Some(b) = elem.bounds {
            let cx = (b[0] + b[2]) / 2;
            let cy = (b[1] + b[3]) / 2;
            // Fall back to raw input tap — agent /click may not accept bounds directly.
            let _ = adb_shell(dev_arg, &["input", "tap", &cx.to_string(), &cy.to_string()]);
            String::new()
        } else {
            String::new()
        };
        if !click_body.is_empty() {
            let _ = curl_post(&format!("{base}/click"), &click_body);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Crash check after tap
        if !is_app_alive(dev_arg, &args.package) {
            eprintln!("  CRASH after tapping '{}'", elem.content);
            state.quirks.push(Quirk {
                screen: screen_id.clone(), quirk_type: "crash".into(),
                description: format!("crash after tapping '{}'", elem.content),
            });
            let _ = adb_shell(dev_arg, &["am", "start", "-W", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
            let _ = wait_for_foreground(dev_arg, &args.package, 5_000);
            forward_agent_port(dev_arg);
            screens_to_explore.push(screen_id);
            continue;
        }

        // Check what screen we're on now
        let new_activity = get_activity(dev_arg);
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
                state.edges.push(NavEdge { from: screen_id.clone(), element: elem.content.clone(), to: new_id.clone() });
                screens_to_explore.push(new_id);
            } else if debug {
                eprintln!("  NO-NAV: tap on '{}' did not change screen fingerprint", label);
            }
        }

        // Navigate back
        let _ = adb_shell(dev_arg, &["input", "keyevent", "KEYCODE_BACK"]);
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Re-queue current screen for more tapping
        screens_to_explore.push(screen_id);

        // Save state
        let _ = std::fs::write(&state_path, serde_yaml::to_string(&state).unwrap_or_default());
    }

    // Write outputs
    let _ = std::fs::write(out_dir.join("navigation-graph.yaml"), serde_yaml::to_string(&state.edges).unwrap_or_default());
    let _ = std::fs::write(out_dir.join("quirks.yaml"), serde_yaml::to_string(&state.quirks).unwrap_or_default());
    let _ = std::fs::write(&state_path, serde_yaml::to_string(&state).unwrap_or_default());

    eprintln!("Crawl done: {} screens, {} edges, {} quirks", state.visited.len(), state.edges.len(), state.quirks.len());
    Ok(())
}
