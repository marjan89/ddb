use crate::adb;
use crate::registry::Device;
use super::test_timeout::StepRunner;

#[derive(serde::Deserialize, Clone)]
pub struct Target {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub content_fuzzy: Option<String>,
    #[serde(default)]
    pub clickable_only: Option<bool>,
    #[serde(default)]
    pub exclude_type: Option<String>,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
}

pub fn find_element(dev: Option<&Device>, target: &Target) -> Result<(i32, i32, String), String> {
    if let (Some(x), Some(y)) = (target.x, target.y) {
        return Ok((x, y, format!("position ({x}, {y})")));
    }

    let yaml = fetch_agent_yaml(dev)?;

    let search_id = target.id.as_deref().unwrap_or("");
    let search_text = target.text.as_deref().unwrap_or("");
    let search_fuzzy = target.content_fuzzy.as_deref().unwrap_or("");

    let mut fuzzy_candidate: Option<(i32, i32, String)> = None;
    let mut fuzzy_clickable = false;

    for chunk in yaml.split("\n- ") {
        let id_match = !search_id.is_empty() && (
            chunk.contains(&format!("platform_id: \"{}\"", search_id))
            || chunk.contains(&format!("id: \"{}\"", search_id))
            || chunk.contains(&format!("platform_id: {}", search_id))
            || chunk.contains(&format!("id: {}", search_id))
        );
        let text_match = !search_text.is_empty() && (
            chunk.contains(&format!("content: \"{}\"", search_text))
            || chunk.contains(&format!("content: {}", search_text))
            || chunk.contains(search_text)
        );
        let fuzzy_match = !search_fuzzy.is_empty() && {
            let needle = search_fuzzy.to_lowercase();
            chunk.lines().any(|line| {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("content:").or_else(|| t.strip_prefix("a11y_label:")) {
                    let hay = rest.to_lowercase();
                    let threshold: f64 = std::env::var("DDB_JACCARD_THRESHOLD")
                        .ok().and_then(|v| v.parse().ok()).unwrap_or(0.6);
                    hay.contains(&needle) || token_jaccard(&needle, &hay) >= threshold
                } else {
                    false
                }
            })
        };

        let exact_match = id_match || text_match;

        if exact_match || fuzzy_match {
            if target.clickable_only == Some(true) && !chunk.contains("clickable: true") {
                continue;
            }
            if let Some(ref exc) = target.exclude_type {
                let type_line = format!("type: {}", exc);
                if chunk.contains(&type_line) {
                    continue;
                }
            }
            let x = extract_yaml_int(chunk, "x: ");
            let y = extract_yaml_int(chunk, "y: ");
            let w = extract_yaml_int(chunk, "w: ");
            let h = extract_yaml_int(chunk, "h: ");

            if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                let (cx, cy) = if chunk.contains("tap_target:") {
                    let tx = extract_yaml_int_after(chunk, "tap_target:", "x: ");
                    let ty = extract_yaml_int_after(chunk, "tap_target:", "y: ");
                    let tw = extract_yaml_int_after(chunk, "tap_target:", "w: ");
                    let th = extract_yaml_int_after(chunk, "tap_target:", "h: ");
                    if let (Some(tx), Some(ty), Some(tw), Some(th)) = (tx, ty, tw, th) {
                        (tx + tw / 2, ty + th / 2)
                    } else {
                        (x + w / 2, y + h / 2)
                    }
                } else {
                    (x + w / 2, y + h / 2)
                };

                let content_line = chunk.lines()
                    .find(|l| l.trim().starts_with("content:"))
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default();
                let desc = format!("{} at ({}, {})", content_line, cx, cy);

                if exact_match {
                    return Ok((cx, cy, desc));
                }
                let is_clickable = chunk.contains("clickable: true");
                let chunk_lower = chunk.to_lowercase();
                let deprioritize = std::env::var("DDB_DEPRIORITIZE_PATTERNS")
                    .unwrap_or_else(|_| "see all,inspiration".into());
                let is_nav_link = deprioritize.split(',')
                    .any(|p| chunk_lower.contains(p.trim()));
                let is_better = match (&fuzzy_candidate, is_clickable, fuzzy_clickable) {
                    (None, _, _) => true,
                    (_, true, false) => true,
                    _ if !is_nav_link => true,
                    _ => false,
                };
                if is_better {
                    fuzzy_candidate = Some((cx, cy, desc));
                    fuzzy_clickable = is_clickable;
                }
            }
        }
    }

    if let Some(candidate) = fuzzy_candidate {
        return Ok(candidate);
    }

    // Fallback: uiautomator dump
    let ui_xml = fetch_ui_dump(dev);
    if !search_fuzzy.is_empty() && ui_xml.to_lowercase().contains(&search_fuzzy.to_lowercase()) {
        if let Some(bounds) = extract_ui_bounds_fuzzy(&ui_xml, search_fuzzy) {
            return Ok((bounds.0, bounds.1, format!("uiautomator: {search_fuzzy}")));
        }
    }
    if !search_id.is_empty() && ui_xml.contains(search_id) {
        if let Some(bounds) = extract_ui_bounds(&ui_xml, search_id) {
            return Ok((bounds.0, bounds.1, format!("uiautomator: {search_id}")));
        }
    }

    let desc = if !search_id.is_empty() { search_id }
        else if !search_text.is_empty() { search_text }
        else { search_fuzzy };
    Err(format!("element not found: {desc}"))
}

pub fn check_element_sources(dev: Option<&Device>, fuzzy: Option<&str>, id: Option<&str>, expected_text: Option<&str>, runner: Option<&StepRunner>) -> Option<String> {
    let ui_xml = if let Some(r) = runner {
        let _ = r.adb_shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]);
        r.adb_shell(dev, &["cat", "/sdcard/ui.xml"]).unwrap_or_default()
    } else {
        fetch_ui_dump(dev)
    };
    let ui_lower = ui_xml.to_lowercase();
    let found_ui = fuzzy.map(|f| ui_lower.contains(&f.to_lowercase())).unwrap_or(false)
        || id.map(|i| ui_xml.contains(i)).unwrap_or(false)
        || expected_text.map(|t| ui_lower.contains(&t.to_lowercase())).unwrap_or(false);
    if found_ui {
        return Some("found in uiautomator".into());
    }
    let a11y_result = if let Some(r) = runner {
        r.adb_shell(dev, &["dumpsys", "activity", "top"])
    } else {
        adb::shell(dev, &["dumpsys", "activity", "top"])
    };
    if let Ok(a11y_dump) = a11y_result {
        let a11y_lower = a11y_dump.to_lowercase();
        if fuzzy.map(|f| a11y_lower.contains(&f.to_lowercase())).unwrap_or(false) {
            return Some("found in activity dump".into());
        }
    }
    if let Ok(elements) = get_semantic_elements(dev) {
        let found = elements.iter().any(|e| {
            let e_lower = e.to_lowercase();
            fuzzy.map(|f| e_lower.contains(&f.to_lowercase())).unwrap_or(false)
                || id.map(|i| e.contains(i)).unwrap_or(false)
        });
        if found {
            let content = elements.iter()
                .find(|e| fuzzy.map(|f| e.to_lowercase().contains(&f.to_lowercase())).unwrap_or(false))
                .and_then(|e| e.lines().find(|l| l.trim().starts_with("content:")).map(|l| l.trim().to_string()))
                .unwrap_or_default();
            return Some(format!("found: {content}"));
        }
    }
    None
}

// --- Bounds parsing ---

pub fn parse_bounds_center(chunk: &str) -> Option<(i32, i32)> {
    let b_start = chunk.find("bounds=\"[")?;
    let bounds_str = &chunk[b_start + 9..];
    let b_end = bounds_str.find(']')?;
    let coords: Vec<&str> = bounds_str[..b_end].split(',').collect();
    if coords.len() != 2 { return None; }
    let x1: i32 = coords[0].parse().ok()?;
    let y1: i32 = coords[1].parse().ok()?;
    let rest = &bounds_str[b_end + 2..];
    let b2 = rest.find(']')?;
    let c2: Vec<&str> = rest[..b2].split(',').collect();
    if c2.len() != 2 { return None; }
    let x2: i32 = c2[0].parse().ok()?;
    let y2: i32 = c2[1].parse().ok()?;
    Some(((x1 + x2) / 2, (y1 + y2) / 2))
}

pub fn extract_ui_bounds(xml: &str, resource_id: &str) -> Option<(i32, i32)> {
    let id_pattern = format!("id/{}", resource_id);
    let idx = xml.find(&id_pattern)?;
    let chunk = &xml[idx..xml.len().min(idx + 200)];
    parse_bounds_center(chunk)
}

pub fn extract_ui_text_bounds(xml: &str, text: &str) -> Option<(i32, i32)> {
    let pattern = format!("text=\"{}", text);
    let idx = xml.find(&pattern)?;
    let chunk = &xml[idx..xml.len().min(idx + 300)];
    parse_bounds_center(chunk)
}

pub fn extract_ui_bounds_fuzzy(xml: &str, fuzzy: &str) -> Option<(i32, i32)> {
    let lower = xml.to_lowercase();
    let needle = fuzzy.to_lowercase();
    let idx = lower.find(&needle)?;
    let chunk = &xml[idx..xml.len().min(idx + 300)];
    if let Some(result) = parse_bounds_center(chunk) {
        return Some(result);
    }
    let before = &xml[..idx + needle.len()];
    let node_start = before.rfind('<')?;
    let node = &xml[node_start..xml.len().min(idx + 500)];
    parse_bounds_center(node)
}

// --- Agent communication ---

pub fn agent_base_url() -> String {
    let port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    format!("http://127.0.0.1:{port}")
}

pub fn fetch_ui_dump(dev: Option<&Device>) -> String {
    let _ = adb::shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]);
    match adb::shell(dev, &["cat", "/sdcard/ui.xml"]) {
        Ok(s) if !s.trim().is_empty() => s,
        _ => String::new(),
    }
}

pub fn fetch_agent_yaml(dev: Option<&Device>) -> Result<String, String> {
    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "2", "--max-time", "10", &format!("{}/semantic", agent_base_url())])
        .output()
        .map_err(|e| format!("curl error: {e}"))?;
    let body = String::from_utf8_lossy(&resp.stdout).to_string();
    if body.is_empty() { Err("semantic agent: empty response".into()) } else { Ok(body) }
}

pub fn fetch_agent_yaml_full(dev: Option<&Device>) -> Result<String, String> {
    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "5", "--max-time", "15", &format!("{}/semantic?scroll=0", agent_base_url())])
        .output()
        .map_err(|e| format!("curl error: {e}"))?;
    let body = String::from_utf8_lossy(&resp.stdout).to_string();
    if body.is_empty() { Err("semantic agent: empty response".into()) } else { Ok(body) }
}

pub fn fetch_agent_yaml_full_with_retry(dev: Option<&Device>) -> Result<String, String> {
    for _ in 0..3 {
        if let Ok(yaml) = fetch_agent_yaml_full(dev) {
            if yaml.contains("- type:") {
                return Ok(yaml);
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    fetch_agent_yaml_full(dev)
}

pub fn get_semantic_elements(dev: Option<&Device>) -> Result<Vec<String>, String> {
    let yaml = fetch_agent_yaml(dev)?;
    Ok(yaml.split("\n- ").map(|s| s.to_string()).collect())
}

// --- YAML parsing helpers ---

pub fn extract_yaml_int(chunk: &str, key: &str) -> Option<i32> {
    for line in chunk.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            return rest.trim().parse().ok();
        }
    }
    None
}

pub fn extract_yaml_int_after(chunk: &str, section: &str, key: &str) -> Option<i32> {
    let section_pos = chunk.find(section)?;
    let after = &chunk[section_pos..];
    for line in after.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix(key) {
            return rest.trim().parse().ok();
        }
    }
    None
}

pub fn token_jaccard(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let a_tokens: HashSet<&str> = a.split_whitespace().collect();
    let b_tokens: HashSet<&str> = b.split_whitespace().collect();
    let intersection = a_tokens.intersection(&b_tokens).count() as f64;
    let union = a_tokens.union(&b_tokens).count() as f64;
    if union == 0.0 { return 0.0; }
    intersection / union
}

pub fn compute_scroll_bounds(dev: Option<&Device>, dir: &str) -> (i32, i32, i32, i32) {
    if let Ok(yaml) = fetch_agent_yaml(dev) {
        for chunk in yaml.split("\n- ") {
            let is_scrollable = chunk.contains("type: container") && (
                chunk.to_lowercase().contains("recyclerview")
                || chunk.to_lowercase().contains("nestedscrollview")
                || chunk.to_lowercase().contains("scrollview")
            );
            if is_scrollable {
                let x = extract_yaml_int(chunk, "x: ").unwrap_or(0);
                let y = extract_yaml_int(chunk, "y: ").unwrap_or(0);
                let w = extract_yaml_int(chunk, "w: ").unwrap_or(0);
                let h = extract_yaml_int(chunk, "h: ").unwrap_or(0);
                if w > 100 && h > 500 {
                    let cx = x + w / 2;
                    let margin = h / 4;
                    return match dir {
                        "up" => (cx, y + margin, cx, y + h - margin),
                        _ => (cx, y + h - margin, cx, y + margin),
                    };
                }
            }
        }
    }
    match dir {
        "up" => (540, 500, 540, 1500),
        _ => (540, 1500, 540, 500),
    }
}

pub fn scroll_direction(dev: Option<&Device>, dir: &str, runner: Option<&StepRunner>) -> Result<(), String> {
    let (x1, y1, x2, y2) = compute_scroll_bounds(dev, dir);
    let x1s = x1.to_string(); let y1s = y1.to_string();
    let x2s = x2.to_string(); let y2s = y2.to_string();
    if let Some(r) = runner {
        r.adb_shell(dev, &["input", "swipe", &x1s, &y1s, &x2s, &y2s, "500"])?;
    } else {
        adb::shell(dev, &["input", "swipe", &x1s, &y1s, &x2s, &y2s, "500"])?;
    }
    Ok(())
}

// --- Unified Element Search (Phase 4B) ---

#[derive(Clone, Debug, PartialEq)]
pub enum ElementSource {
    IdleBarrier { timeout_s: u64, resources: Vec<String> },
    Semantic,
    UIAutomator,
    Activity,
}

pub const DEFAULT_SOURCES: &[ElementSource] = &[
    ElementSource::Semantic,
    ElementSource::UIAutomator,
];

pub fn idle_barrier_sources(timeout_s: u64) -> Vec<ElementSource> {
    vec![
        ElementSource::IdleBarrier {
            timeout_s,
            resources: vec!["ui_thread".into(), "network".into(), "scroll".into(), "layout".into(), "presentation".into()],
        },
        ElementSource::Semantic,
        ElementSource::UIAutomator,
    ]
}

pub fn idle_barrier_network_only(timeout_s: u64) -> Vec<ElementSource> {
    vec![
        ElementSource::IdleBarrier {
            timeout_s,
            resources: vec!["network".into()],
        },
        ElementSource::Semantic,
        ElementSource::UIAutomator,
    ]
}

pub fn find_element_unified(
    dev: Option<&Device>,
    target: &Target,
    sources: &[ElementSource],
    runner: Option<&StepRunner>,
) -> Result<(i32, i32, String), String> {
    if let (Some(x), Some(y)) = (target.x, target.y) {
        return Ok((x, y, format!("position ({x}, {y})")));
    }

    let search_id = target.id.as_deref().unwrap_or("");
    let search_text = target.text.as_deref().unwrap_or("");
    let search_fuzzy = target.content_fuzzy.as_deref().unwrap_or("");

    for source in sources {
        match source {
            ElementSource::IdleBarrier { timeout_s, resources } => {
                if let Some(result) = query_when_idle(target, *timeout_s, resources, runner) {
                    return Ok(result);
                }
            }
            ElementSource::Semantic => {
                let yaml_result = if let Some(r) = runner {
                    r.curl_with_deadline(&format!("{}/semantic", agent_base_url()), "GET", None)
                } else {
                    fetch_agent_yaml(dev)
                };
                if let Ok(yaml) = yaml_result {
                    if let Some(result) = search_semantic_yaml(
                        &yaml, target, search_id, search_text, search_fuzzy,
                    ) {
                        return Ok(result);
                    }
                }
            }
            ElementSource::UIAutomator => {
                let ui_xml = if let Some(r) = runner {
                    let _ = r.adb_shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]);
                    r.adb_shell(dev, &["cat", "/sdcard/ui.xml"]).unwrap_or_default()
                } else {
                    fetch_ui_dump(dev)
                };
                if let Some(result) = search_uiautomator(
                    &ui_xml, search_id, search_text, search_fuzzy,
                ) {
                    return Ok(result);
                }
            }
            ElementSource::Activity => {
                let dump_result = if let Some(r) = runner {
                    r.adb_shell(dev, &["dumpsys", "activity", "top"])
                } else {
                    adb::shell(dev, &["dumpsys", "activity", "top"])
                };
                if let Ok(dump) = dump_result {
                    let lower = dump.to_lowercase();
                    if !search_fuzzy.is_empty() && lower.contains(&search_fuzzy.to_lowercase()) {
                        return Ok((540, 1200, format!("activity dump: {search_fuzzy}")));
                    }
                }
            }
        }
    }

    let desc = if !search_id.is_empty() { search_id }
        else if !search_text.is_empty() { search_text }
        else { search_fuzzy };
    Err(format!("element not found: {desc}"))
}

fn search_semantic_yaml(
    yaml: &str,
    target: &Target,
    search_id: &str,
    search_text: &str,
    search_fuzzy: &str,
) -> Option<(i32, i32, String)> {
    let mut fuzzy_candidate: Option<(i32, i32, String)> = None;
    let mut fuzzy_clickable = false;

    for chunk in yaml.split("\n- ") {
        let id_match = !search_id.is_empty() && (
            chunk.contains(&format!("platform_id: \"{}\"", search_id))
            || chunk.contains(&format!("id: \"{}\"", search_id))
            || chunk.contains(&format!("platform_id: {}", search_id))
            || chunk.contains(&format!("id: {}", search_id))
        );
        let text_match = !search_text.is_empty() && (
            chunk.contains(&format!("content: \"{}\"", search_text))
            || chunk.contains(&format!("content: {}", search_text))
            || chunk.contains(search_text)
        );
        let fuzzy_match = !search_fuzzy.is_empty() && {
            let needle = search_fuzzy.to_lowercase();
            chunk.lines().any(|line| {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("content:").or_else(|| t.strip_prefix("a11y_label:")) {
                    let hay = rest.to_lowercase();
                    let threshold: f64 = std::env::var("DDB_JACCARD_THRESHOLD")
                        .ok().and_then(|v| v.parse().ok()).unwrap_or(0.6);
                    hay.contains(&needle) || token_jaccard(&needle, &hay) >= threshold
                } else {
                    false
                }
            })
        };

        let exact_match = id_match || text_match;

        if exact_match || fuzzy_match {
            if target.clickable_only == Some(true) && !chunk.contains("clickable: true") {
                continue;
            }
            if let Some(ref exc) = target.exclude_type {
                if chunk.contains(&format!("type: {}", exc)) {
                    continue;
                }
            }
            let x = extract_yaml_int(chunk, "x: ");
            let y = extract_yaml_int(chunk, "y: ");
            let w = extract_yaml_int(chunk, "w: ");
            let h = extract_yaml_int(chunk, "h: ");

            if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                let (cx, cy) = if chunk.contains("tap_target:") {
                    let tx = extract_yaml_int_after(chunk, "tap_target:", "x: ");
                    let ty = extract_yaml_int_after(chunk, "tap_target:", "y: ");
                    let tw = extract_yaml_int_after(chunk, "tap_target:", "w: ");
                    let th = extract_yaml_int_after(chunk, "tap_target:", "h: ");
                    if let (Some(tx), Some(ty), Some(tw), Some(th)) = (tx, ty, tw, th) {
                        (tx + tw / 2, ty + th / 2)
                    } else {
                        (x + w / 2, y + h / 2)
                    }
                } else {
                    (x + w / 2, y + h / 2)
                };

                let content_line = chunk.lines()
                    .find(|l| l.trim().starts_with("content:"))
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default();
                let desc = format!("{} at ({}, {})", content_line, cx, cy);

                if exact_match {
                    return Some((cx, cy, desc));
                }
                let is_clickable = chunk.contains("clickable: true");
                let chunk_lower = chunk.to_lowercase();
                let deprioritize = std::env::var("DDB_DEPRIORITIZE_PATTERNS")
                    .unwrap_or_else(|_| "see all,inspiration".into());
                let is_nav_link = deprioritize.split(',')
                    .any(|p| chunk_lower.contains(p.trim()));
                let is_better = match (&fuzzy_candidate, is_clickable, fuzzy_clickable) {
                    (None, _, _) => true,
                    (_, true, false) => true,
                    _ if !is_nav_link => true,
                    _ => false,
                };
                if is_better {
                    fuzzy_candidate = Some((cx, cy, desc));
                    fuzzy_clickable = is_clickable;
                }
            }
        }
    }

    fuzzy_candidate
}

fn search_uiautomator(
    ui_xml: &str,
    search_id: &str,
    _search_text: &str,
    search_fuzzy: &str,
) -> Option<(i32, i32, String)> {
    if !search_fuzzy.is_empty() && ui_xml.to_lowercase().contains(&search_fuzzy.to_lowercase()) {
        if let Some(bounds) = extract_ui_bounds_fuzzy(ui_xml, search_fuzzy) {
            return Some((bounds.0, bounds.1, format!("uiautomator: {search_fuzzy}")));
        }
    }
    if !search_id.is_empty() && ui_xml.contains(search_id) {
        if let Some(bounds) = extract_ui_bounds(ui_xml, search_id) {
            return Some((bounds.0, bounds.1, format!("uiautomator: {search_id}")));
        }
    }
    None
}

// --- Idle Barrier (Phase 5C) ---

fn query_when_idle(target: &Target, timeout_s: u64, resources: &[String], runner: Option<&StepRunner>) -> Option<(i32, i32, String)> {
    let match_obj = build_match_json(target);
    let body = serde_json::json!({
        "match": match_obj,
        "idle_resources": resources,
        "timeout": timeout_s,
    });
    let body_str = body.to_string();
    let url = format!("{}/query-when-idle", agent_base_url());

    for attempt in 0..3 {
        let resp_body = if let Some(r) = runner {
            match r.curl_with_deadline(&url, "POST", Some(&body_str)) {
                Ok(s) => s,
                Err(_) => return None,
            }
        } else {
            let resp = std::process::Command::new("curl")
                .args([
                    "-s", "--connect-timeout", "2",
                    "--max-time", &(timeout_s + 2).to_string(),
                    "-X", "POST",
                    "-H", "Content-Type: application/json",
                    "-d", &body_str,
                    &url,
                ])
                .output()
                .ok()?;
            String::from_utf8_lossy(&resp.stdout).into_owned()
        };

        if resp_body.is_empty() || resp_body.contains("404") {
            return None;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp_body) {
            if json.get("found") == Some(&serde_json::Value::Bool(true)) {
                let element = json.get("element");
                let x = element.and_then(|e| e.get("x")).and_then(|v| v.as_i64()).unwrap_or(540) as i32;
                let y = element.and_then(|e| e.get("y")).and_then(|v| v.as_i64()).unwrap_or(1200) as i32;
                let content = element.and_then(|e| e.get("content")).and_then(|v| v.as_str()).unwrap_or("");
                let wait_ms = json.get("idle_wait_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                return Some((x, y, format!("idle-barrier: {} at ({},{}) wait={}ms", content, x, y, wait_ms)));
            }
            if json.get("timeout") == Some(&serde_json::Value::Bool(true)) {
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
                return None;
            }
        }
        return None;
    }
    None
}

pub fn scroll_search(target: &Target, max_scroll: u32, restore_scroll: bool, runner: Option<&StepRunner>) -> Option<(i32, i32, String)> {
    let match_obj = build_match_json(target);
    let body = serde_json::json!({
        "match": match_obj,
        "max_scroll": max_scroll,
        "idle_resources": ["network"],
        "restore_scroll": restore_scroll,
    });
    let body_str = body.to_string();
    let url = format!("{}/scroll-search", agent_base_url());

    let resp_body = if let Some(r) = runner {
        r.curl_with_deadline(&url, "POST", Some(&body_str)).ok()?
    } else {
        let max_time = max_scroll as u64 * 2 + 10;
        let resp = std::process::Command::new("curl")
            .args([
                "-s", "--connect-timeout", "3",
                "--max-time", &max_time.to_string(),
                "-X", "POST",
                "-H", "Content-Type: application/json",
                "-d", &body_str,
                &url,
            ])
            .output()
            .ok()?;
        String::from_utf8_lossy(&resp.stdout).into_owned()
    };
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp_body) {
        if json.get("found") == Some(&serde_json::Value::Bool(true)) {
            let element = json.get("element");
            let bounds = element.and_then(|e| e.get("bounds"));
            let bx = bounds.and_then(|b| b.get("x")).and_then(|v| v.as_i64()).unwrap_or(540) as i32;
            let by = bounds.and_then(|b| b.get("y")).and_then(|v| v.as_i64()).unwrap_or(1200) as i32;
            let bw = bounds.and_then(|b| b.get("w")).and_then(|v| v.as_i64()).unwrap_or(100) as i32;
            let bh = bounds.and_then(|b| b.get("h")).and_then(|v| v.as_i64()).unwrap_or(50) as i32;
            let x = bx + bw / 2;
            let y = by + bh / 2;
            let content = element.and_then(|e| e.get("content")).and_then(|v| v.as_str()).unwrap_or("");
            let scrolls = json.get("scrolls").and_then(|v| v.as_u64()).unwrap_or(0);
            return Some((x, y, format!("scroll-search: {} at ({},{}) scrolls={}", content, x, y, scrolls)));
        }
    }
    None
}

fn build_match_json(target: &Target) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(ref id) = target.id {
        obj.insert("id".into(), serde_json::Value::String(id.clone()));
    }
    if let Some(ref text) = target.text {
        obj.insert("text".into(), serde_json::Value::String(text.clone()));
    }
    if let Some(ref fuzzy) = target.content_fuzzy {
        obj.insert("content_fuzzy".into(), serde_json::Value::String(fuzzy.clone()));
    }
    serde_json::Value::Object(obj)
}
