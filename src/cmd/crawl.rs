use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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
    adb_shell(dev, &["dumpsys", "activity", "top"])
        .ok()
        .and_then(|out| {
            out.lines().find(|l| l.contains("ACTIVITY"))
                .and_then(|l| l.split_whitespace().nth(1))
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".into())
}

fn is_app_alive(dev: Option<&str>, pkg: &str) -> bool {
    adb_shell(dev, &["pidof", pkg]).map(|s| !s.trim().is_empty()).unwrap_or(false)
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
    let mut elements = Vec::new();
    let mut content = String::new();
    let mut etype = String::new();
    let mut eid: Option<String> = None;
    let mut clickable = false;
    let mut in_element = false;

    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- content:") {
            if in_element && (!content.is_empty() || !etype.is_empty()) {
                elements.push(CrawlElement { content: content.clone(), element_type: etype.clone(), id: eid.take(), bounds: None, clickable });
            }
            content = trimmed.strip_prefix("- content:").unwrap_or("").trim().trim_matches('"').to_string();
            etype.clear(); eid = None; clickable = false; in_element = true;
        } else if in_element {
            if trimmed.starts_with("type:") {
                etype = trimmed.strip_prefix("type:").unwrap_or("").trim().trim_matches('"').to_string();
            } else if trimmed.starts_with("platform_id:") {
                let v = trimmed.split(':').nth(1).unwrap_or("").trim().trim_matches('"');
                if !v.is_empty() { eid = Some(v.to_string()); }
            } else if trimmed.starts_with("clickable:") {
                clickable = trimmed.contains("true");
            }
        }
    }
    if in_element && (!content.is_empty() || !etype.is_empty()) {
        elements.push(CrawlElement { content, element_type: etype, id: eid, bounds: None, clickable });
    }
    elements
}

fn scroll_and_discover(dev: Option<&str>, base: &str, max_depth: usize) -> Vec<CrawlElement> {
    let mut all_elements = Vec::new();
    let mut seen_contents: HashSet<String> = HashSet::new();

    for _ in 0..max_depth {
        if let Ok(sem) = curl_get(&format!("{base}/semantic")) {
            let elems = parse_semantic_elements(&sem);
            let mut new_count = 0;
            for e in &elems {
                if seen_contents.insert(e.content.clone()) {
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

    // Launch app
    let _ = adb_shell(dev_name, &["am", "force-stop", &args.package]);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let main_activity = std::env::var("DDB_MAIN_ACTIVITY").unwrap_or_else(|_| format!("{}/.MainActivity", args.package));
    let _ = adb_shell(dev_name, &["am", "start", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Wait for agent
    let mut ready = false;
    for _ in 0..10 {
        if curl_get(&format!("{base}/health")).map(|b| b.contains("semantic-agent")).unwrap_or(false) { ready = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !ready { return Err("agent not ready".into()); }

    let mut screens_to_explore: Vec<String> = vec!["initial".into()];

    while let Some(_) = screens_to_explore.pop() {
        if state.visited.len() >= args.max_screens { break; }

        // Crash detection
        if !is_app_alive(dev_name, &args.package) {
            eprintln!("  CRASH detected — relaunching");
            state.quirks.push(Quirk { screen: "unknown".into(), quirk_type: "crash".into(), description: "app crashed during crawl".into() });
            let _ = adb_shell(dev_name, &["am", "start", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
            std::thread::sleep(std::time::Duration::from_secs(3));
            continue;
        }

        // Get current screen
        let semantic = match curl_get(&format!("{base}/semantic")) {
            Ok(s) => s,
            Err(_) => { eprintln!("  /semantic unreachable"); break; }
        };

        let activity = get_activity(dev_name);
        let elements = parse_semantic_elements(&semantic);

        // Scroll discovery
        let all_elements = if args.max_scroll_depth > 0 {
            scroll_and_discover(dev_name, &base, args.max_scroll_depth)
        } else { elements.clone() };

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
            let ss_name = if take_screenshot(dev_name, ss_path.to_str().unwrap_or("")) {
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

        // Find untapped clickable elements
        let tappable: Vec<&CrawlElement> = all_elements.iter()
            .filter(|e| e.clickable && !e.content.is_empty())
            .filter(|e| !exclude.iter().any(|p| e.content.to_lowercase().contains(p)))
            .filter(|e| state.visited.get(&screen_id).map_or(true, |s| !s.tapped.contains(&e.content)))
            .collect();

        if tappable.is_empty() { continue; }

        let elem = tappable[0];
        eprintln!("  TAP: '{}'", elem.content);

        // Record as tapped
        if let Some(s) = state.visited.get_mut(&screen_id) { s.tapped.push(elem.content.clone()); }

        // Execute tap via /click
        let click_body = if let Some(ref id) = elem.id {
            serde_json::json!({"resource_id": id}).to_string()
        } else {
            serde_json::json!({"content_fuzzy": elem.content}).to_string()
        };
        let _ = curl_post(&format!("{base}/click"), &click_body);
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Crash check after tap
        if !is_app_alive(dev_name, &args.package) {
            eprintln!("  CRASH after tapping '{}'", elem.content);
            state.quirks.push(Quirk {
                screen: screen_id.clone(), quirk_type: "crash".into(),
                description: format!("crash after tapping '{}'", elem.content),
            });
            let _ = adb_shell(dev_name, &["am", "start", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.LAUNCHER", "-n", &main_activity]);
            std::thread::sleep(std::time::Duration::from_secs(3));
            screens_to_explore.push(screen_id);
            continue;
        }

        // Check what screen we're on now
        let new_activity = get_activity(dev_name);
        if let Ok(new_sem) = curl_get(&format!("{base}/semantic")) {
            let new_elems = parse_semantic_elements(&new_sem);
            let new_id = fingerprint(&new_activity, &new_elems);

            if new_id != screen_id {
                state.edges.push(NavEdge { from: screen_id.clone(), element: elem.content.clone(), to: new_id.clone() });
                screens_to_explore.push(new_id);
            }
        }

        // Navigate back
        let _ = adb_shell(dev_name, &["input", "keyevent", "KEYCODE_BACK"]);
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
