use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct CrawlArgs {
    /// App package name
    #[arg(long, env = "DDB_TEST_PACKAGE")]
    pub package: String,

    /// Output directory for crawl results
    #[arg(long, default_value = "catalogue/crawl")]
    pub output: String,

    /// Resume from previous crawl state
    #[arg(long)]
    pub resume: bool,

    /// Max screens to discover
    #[arg(long, default_value = "50")]
    pub max_screens: usize,

    /// Max scroll depth per screen
    #[arg(long, default_value = "10")]
    pub max_scroll_depth: usize,

    /// Elements to never tap
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
struct NavEdge {
    from: String,
    element: String,
    to: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Quirk {
    screen: String,
    quirk_type: String,
    description: String,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct CrawlState {
    visited: HashMap<String, ScreenSnapshot>,
    edges: Vec<NavEdge>,
    quirks: Vec<Quirk>,
    back_stack: Vec<String>,
}

fn fingerprint(activity: &str, elements: &[CrawlElement]) -> String {
    let mut set: Vec<String> = elements.iter()
        .map(|e| format!("{}:{}", e.id.as_deref().unwrap_or(&e.content), e.element_type))
        .collect();
    set.push(format!("activity:{}", activity));
    set.sort();
    let hash = set.join("|");
    format!("{:x}", md5_hash(&hash))
}

fn md5_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() { return 1.0; }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 { return 0.0; }
    intersection as f64 / union as f64
}

fn parse_semantic_elements(yaml: &str) -> Vec<CrawlElement> {
    let mut elements = Vec::new();
    let mut current_content = String::new();
    let mut current_type = String::new();
    let mut current_id: Option<String> = None;
    let mut current_clickable = false;

    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- content:") || trimmed.starts_with("content:") {
            if !current_content.is_empty() || !current_type.is_empty() {
                elements.push(CrawlElement {
                    content: current_content.clone(),
                    element_type: current_type.clone(),
                    id: current_id.take(),
                    bounds: None,
                    clickable: current_clickable,
                });
            }
            current_content = trimmed.trim_start_matches("- content:").trim_start_matches("content:").trim().trim_matches('"').to_string();
            current_type.clear();
            current_clickable = false;
        } else if trimmed.starts_with("type:") {
            current_type = trimmed.strip_prefix("type:").unwrap_or("").trim().trim_matches('"').to_string();
        } else if trimmed.starts_with("platform_id:") || trimmed.starts_with("id:") {
            let id_str = trimmed.split(':').nth(1).unwrap_or("").trim().trim_matches('"');
            if !id_str.is_empty() {
                current_id = Some(id_str.to_string());
            }
        } else if trimmed.starts_with("clickable:") {
            current_clickable = trimmed.contains("true");
        }
    }
    if !current_content.is_empty() || !current_type.is_empty() {
        elements.push(CrawlElement {
            content: current_content,
            element_type: current_type,
            id: current_id,
            bounds: None,
            clickable: current_clickable,
        });
    }
    elements
}

fn agent_base_url() -> String {
    let port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    format!("http://127.0.0.1:{port}")
}

fn curl_get(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "10", url])
        .output()
        .map_err(|e| format!("curl failed: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err("curl returned non-zero".into())
    }
}

pub fn run(dev_name: Option<&str>, args: CrawlArgs) -> Result<(), String> {
    let out_dir = PathBuf::from(&args.output);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("create output dir: {e}"))?;
    std::fs::create_dir_all(out_dir.join("screens")).map_err(|e| format!("create screens dir: {e}"))?;
    std::fs::create_dir_all(out_dir.join("screenshots")).map_err(|e| format!("create screenshots dir: {e}"))?;

    let state_path = out_dir.join("crawl-state.yaml");
    let mut state: CrawlState = if args.resume && state_path.exists() {
        let content = std::fs::read_to_string(&state_path).map_err(|e| format!("read state: {e}"))?;
        serde_yaml::from_str(&content).unwrap_or_default()
    } else {
        CrawlState::default()
    };

    let exclude_patterns: Vec<String> = args.exclude.split(',').map(|s| s.trim().to_lowercase()).collect();
    let base = agent_base_url();

    eprintln!("Starting crawl of {} (max {} screens)", args.package, args.max_screens);

    let mut queue: Vec<String> = vec!["launch".into()];
    let mut visit_counts: HashMap<String, usize> = HashMap::new();

    while let Some(_) = queue.pop() {
        if state.visited.len() >= args.max_screens {
            eprintln!("  max screens ({}) reached", args.max_screens);
            break;
        }

        let semantic = match curl_get(&format!("{base}/semantic")) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("  /semantic unreachable — app may have crashed");
                break;
            }
        };

        let elements = parse_semantic_elements(&semantic);
        let activity = "unknown"; // TODO: get from adb dumpsys
        let screen_id = fingerprint(activity, &elements);

        let count = visit_counts.entry(screen_id.clone()).or_insert(0);
        *count += 1;
        if *count > 3 {
            eprintln!("  screen {} visited 3 times, skipping", screen_id);
            continue;
        }

        if !state.visited.contains_key(&screen_id) {
            eprintln!("  NEW screen: {} ({} elements)", screen_id, elements.len());

            let snapshot = ScreenSnapshot {
                screen_id: screen_id.clone(),
                activity: activity.to_string(),
                elements: elements.clone(),
                scroll_depth: 0,
                tapped: Vec::new(),
                screenshot: None,
            };

            // Save screen
            let screen_yaml = serde_yaml::to_string(&snapshot).unwrap_or_default();
            let _ = std::fs::write(out_dir.join("screens").join(format!("{}.yaml", screen_id)), &screen_yaml);

            state.visited.insert(screen_id.clone(), snapshot);
        }

        // Find tappable elements
        let tappable: Vec<&CrawlElement> = elements.iter()
            .filter(|e| e.clickable)
            .filter(|e| !exclude_patterns.iter().any(|p| e.content.to_lowercase().contains(p)))
            .filter(|e| {
                let visited = state.visited.get(&screen_id);
                visited.map_or(true, |s| !s.tapped.contains(&e.content))
            })
            .collect();

        if tappable.is_empty() {
            eprintln!("  no untapped elements on {}", screen_id);
            continue;
        }

        // Tap first untapped element
        let elem = tappable[0];
        eprintln!("  tapping: '{}'", elem.content);

        // TODO: actually tap the element and observe result
        // For now, just record it
        if let Some(snapshot) = state.visited.get_mut(&screen_id) {
            snapshot.tapped.push(elem.content.clone());
        }

        // Save state after each screen
        let state_yaml = serde_yaml::to_string(&state).unwrap_or_default();
        let _ = std::fs::write(&state_path, &state_yaml);

        // Add current screen back to queue for more tapping
        queue.push(screen_id.clone());
    }

    // Write navigation graph
    let graph_yaml = serde_yaml::to_string(&state.edges).unwrap_or_default();
    let _ = std::fs::write(out_dir.join("navigation-graph.yaml"), &graph_yaml);

    // Write quirks
    let quirks_yaml = serde_yaml::to_string(&state.quirks).unwrap_or_default();
    let _ = std::fs::write(out_dir.join("quirks.yaml"), &quirks_yaml);

    eprintln!("Crawl complete: {} screens, {} edges, {} quirks",
        state.visited.len(), state.edges.len(), state.quirks.len());

    Ok(())
}
