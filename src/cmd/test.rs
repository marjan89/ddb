use clap::Args;
use std::path::Path;

use crate::adb;
use crate::registry::{Device, Registry};

#[derive(Args)]
pub struct TestArgs {
    /// YAML test spec file(s)
    pub specs: Vec<String>,

    /// Output report path (JSON)
    #[arg(long)]
    pub report: Option<String>,

    /// Timeout per step in seconds
    #[arg(long, default_value = "10")]
    pub step_timeout: u64,
}

#[derive(serde::Deserialize)]
struct TestSpecRaw {
    id: String,
    name: String,
    #[serde(default)]
    precondition: Option<Precondition>,
    steps: Vec<StepRaw>,
}

struct TestSpec {
    id: String,
    name: String,
    precondition: Option<Precondition>,
    steps: Vec<Step>,
}

#[derive(serde::Deserialize)]
struct Precondition {
    #[serde(default)]
    activity: Option<String>,
    #[serde(default)]
    scroll_to: Option<String>,
}

#[derive(serde::Deserialize)]
struct StepRaw {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    assert: Option<String>,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    catalogue: Option<String>,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

enum Step {
    Action(ActionStep),
    Assert(AssertStep),
}

struct ActionStep {
    action: String,
    target: Option<Target>,
    text: Option<String>,
    direction: Option<String>,
    output: Option<String>,
    catalogue: Option<String>,
}

struct AssertStep {
    assert: String,
    expected: Option<String>,
    target: Option<Target>,
    text: Option<String>,
    hint: Option<String>,
    enabled: Option<bool>,
}

impl StepRaw {
    fn into_step(self) -> Result<Step, String> {
        if let Some(action) = self.action {
            Ok(Step::Action(ActionStep {
                action,
                target: self.target,
                text: self.text,
                direction: self.direction,
                output: self.output,
                catalogue: self.catalogue,
            }))
        } else if let Some(assert) = self.assert {
            Ok(Step::Assert(AssertStep {
                assert,
                expected: self.expected,
                target: self.target,
                text: self.text,
                hint: self.hint,
                enabled: self.enabled,
            }))
        } else {
            Err("step must have either 'action' or 'assert' field".to_string())
        }
    }
}

#[derive(serde::Deserialize)]
struct Target {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(serde::Serialize)]
struct TestResult {
    id: String,
    name: String,
    status: String,
    steps_run: usize,
    steps_total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<FailureDetail>,
}

#[derive(serde::Serialize)]
struct FailureDetail {
    step: usize,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
}

pub fn run(dev_name: Option<&str>, args: TestArgs) -> Result<(), String> {
    let devices = Registry::load()?;
    let dev = if devices.is_empty() && dev_name.is_none() {
        None
    } else {
        let (_, d) = Registry::resolve(dev_name, &devices)?;
        Some(d)
    };

    if args.specs.is_empty() {
        return Err("no test spec files provided".to_string());
    }

    // Set up port forwarding for agent
    if let Some(ref d) = dev {
        let _ = adb::adb(Some(d), &["forward", "tcp:9876", "tcp:9876"]);
    }

    // Disable animations for reliable test execution
    set_animations(false);

    let mut results = Vec::new();
    let mut pass = 0;
    let mut fail = 0;

    for spec_path in &args.specs {
        let content = std::fs::read_to_string(spec_path)
            .map_err(|e| format!("read {spec_path}: {e}"))?;
        let raw: TestSpecRaw = serde_yaml::from_str(&content)
            .map_err(|e| format!("parse {spec_path}: {e}"))?;
        let steps: Vec<Step> = raw.steps.into_iter()
            .map(|s| s.into_step())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid step in {spec_path}: {e}"))?;
        let spec = TestSpec {
            id: raw.id,
            name: raw.name,
            precondition: raw.precondition,
            steps,
        };

        let result = run_spec(&spec, dev.as_ref(), args.step_timeout);

        if result.status == "PASS" {
            pass += 1;
            println!("  PASS  {} — {}", result.id, result.name);
        } else {
            fail += 1;
            let empty = String::new();
            let detail = result.failure.as_ref().map(|f| &f.description).unwrap_or(&empty);
            println!("  FAIL  {} — {} (step {}: {})",
                result.id, result.name,
                result.failure.as_ref().map(|f| f.step).unwrap_or(0),
                detail
            );
        }

        results.push(result);
    }

    // Re-enable animations (always, even on failure)
    set_animations(true);

    println!("\n{} passed, {} failed, {} total", pass, fail, pass + fail);

    if let Some(ref report_path) = args.report {
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| format!("json error: {e}"))?;
        std::fs::write(report_path, &json)
            .map_err(|e| format!("write report: {e}"))?;
        eprintln!("report: {}", report_path);
    }

    if fail > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn set_animations(enabled: bool) {
    let val = if enabled { "1" } else { "0" };
    let _ = std::process::Command::new("curl")
        .args(["-s", "-X", "POST", &format!("http://localhost:9876/animations?enabled={enabled}")])
        .output();
}

fn run_spec(spec: &TestSpec, dev: Option<&Device>, timeout: u64) -> TestResult {
    // Check preconditions
    if let Some(ref pre) = spec.precondition {
        if let Some(ref activity) = pre.activity {
            // Verify current activity
            if let Ok(current) = get_current_activity(dev) {
                if !current.contains(activity) {
                    return TestResult {
                        id: spec.id.clone(),
                        name: spec.name.clone(),
                        status: "FAIL".to_string(),
                        steps_run: 0,
                        steps_total: spec.steps.len(),
                        failure: Some(FailureDetail {
                            step: 0,
                            description: format!("precondition failed: expected activity {activity}, got {current}"),
                            screenshot: None,
                        }),
                    };
                }
            }
        }
        if let Some(ref scroll_target) = pre.scroll_to {
            let _ = scroll_to_element(dev, scroll_target);
        }
    }

    // Wait for idle
    wait_idle(dev, timeout);

    // Run steps
    for (i, step) in spec.steps.iter().enumerate() {
        let result = match step {
            Step::Action(a) => execute_action(dev, a),
            Step::Assert(a) => execute_assert(dev, a, timeout),
        };

        if let Err(err) = result {
            // Capture screenshot on failure
            let screenshot = capture_failure_screenshot(dev, &spec.id, i);
            return TestResult {
                id: spec.id.clone(),
                name: spec.name.clone(),
                status: "FAIL".to_string(),
                steps_run: i,
                steps_total: spec.steps.len(),
                failure: Some(FailureDetail {
                    step: i + 1,
                    description: err,
                    screenshot,
                }),
            };
        }

        // Wait for idle after actions
        if matches!(step, Step::Action(_)) {
            wait_idle(dev, timeout);
        }
    }

    TestResult {
        id: spec.id.clone(),
        name: spec.name.clone(),
        status: "PASS".to_string(),
        steps_run: spec.steps.len(),
        steps_total: spec.steps.len(),
        failure: None,
    }
}

fn execute_action(dev: Option<&Device>, action: &ActionStep) -> Result<(), String> {
    match action.action.as_str() {
        "tap" => {
            let target = action.target.as_ref().ok_or("tap: no target")?;
            let (x, y) = find_element(dev, target)?;
            adb::shell(dev, &["input", "tap", &x.to_string(), &y.to_string()])?;
            Ok(())
        }
        "type" => {
            let text = action.text.as_ref().ok_or("type: no text")?;
            let escaped = text.replace(' ', "%s");
            adb::shell(dev, &["input", "text", &escaped])?;
            Ok(())
        }
        "scroll" | "scroll_to" => {
            if let Some(ref target) = action.target {
                let id_or_text = target.id.as_deref().or(target.text.as_deref()).unwrap_or("");
                scroll_to_element(dev, id_or_text)?;
            } else {
                let dir = action.direction.as_deref().unwrap_or("down");
                scroll_direction(dev, dir)?;
            }
            Ok(())
        }
        "back" => {
            adb::shell(dev, &["input", "keyevent", "4"])?;
            Ok(())
        }
        "home" => {
            adb::shell(dev, &["input", "keyevent", "3"])?;
            Ok(())
        }
        "capture" => {
            let output = action.output.as_ref().ok_or("capture: no output path")?;
            let output_path = std::path::Path::new(output);
            if let Some(parent) = output_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let cat_info = action
                .catalogue
                .as_deref()
                .map(|c| {
                    let key = crate::catalogue::detect_catalogue_path(output).map(|(_, k)| k);
                    (std::path::PathBuf::from(c), key)
                })
                .or_else(|| {
                    crate::catalogue::detect_catalogue_path(output)
                        .map(|(root, key)| (root, Some(key)))
                });

            let history_count = if output_path.exists() {
                crate::catalogue::archive_existing(output_path)?
            } else {
                0
            };

            let yaml = fetch_agent_yaml(dev)?;
            std::fs::write(output, &yaml).map_err(|e| format!("write capture: {e}"))?;
            eprintln!("captured → {output}");

            if let Some((cat_root, Some(entry_key))) = cat_info {
                let schema: crate::semantic::SemanticSchema =
                    serde_yaml::from_str(&yaml)
                        .map_err(|e| format!("count elements: {e}"))?;
                let count = schema.elements.len() as u64;
                crate::catalogue::update_manifest_semantic(&cat_root, &entry_key, count, history_count)?;
            }
            Ok(())
        }
        "capture_screenshot" => {
            let output = action.output.as_ref().ok_or("capture_screenshot: no output path")?;
            let output_path = std::path::Path::new(output);
            if let Some(parent) = output_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let cat_info = action
                .catalogue
                .as_deref()
                .map(|c| {
                    let key = crate::catalogue::detect_catalogue_path(output).map(|(_, k)| k);
                    (std::path::PathBuf::from(c), key)
                })
                .or_else(|| {
                    crate::catalogue::detect_catalogue_path(output)
                        .map(|(root, key)| (root, Some(key)))
                });

            if output_path.exists() {
                let _ = crate::catalogue::archive_existing(output_path);
            }

            let png = adb::adb_raw(dev, &["exec-out", "screencap", "-p"])?;
            std::fs::write(output, &png).map_err(|e| format!("write screenshot: {e}"))?;
            let _ = std::process::Command::new("sips")
                .args(["-Z", "1200", output])
                .output();
            eprintln!("screenshot → {output}");

            if let Some((cat_root, Some(entry_key))) = cat_info {
                let _ = crate::catalogue::update_manifest_screenshot(&cat_root, &entry_key);
            }
            Ok(())
        }
        other => Err(format!("unknown action: {other}")),
    }
}

fn execute_assert(dev: Option<&Device>, assert: &AssertStep, timeout: u64) -> Result<(), String> {
    match assert.assert.as_str() {
        "activity" => {
            let expected = assert.expected.as_ref().ok_or("assert activity: no expected")?;
            let current = get_current_activity(dev)?;
            if current.contains(expected.as_str()) {
                Ok(())
            } else {
                Err(format!("expected activity {expected}, got {current}"))
            }
        }
        "element_exists" => {
            let elements = get_semantic_elements(dev)?;
            let target = assert.target.as_ref();
            let expected_text = assert.text.as_deref();
            let expected_hint = assert.hint.as_deref();

            let found = elements.iter().any(|e| {
                let id_match = target
                    .and_then(|t| t.id.as_deref())
                    .map_or(true, |id| {
                        e.contains(&format!("platform_id: \"{}\"", id))
                        || e.contains(&format!("id: \"{}\"", id))
                        || e.contains(&format!("platform_id: {}", id))
                        || e.contains(&format!("id: {}", id))
                    });

                let text_match = expected_text.map_or(true, |t| {
                    if t.starts_with("contains(") && t.ends_with(')') {
                        let inner = &t[9..t.len() - 1].trim_matches('"');
                        e.contains(inner)
                    } else {
                        e.contains(&format!("content: \"{}\"", t))
                        || e.contains(&format!("content: {}", t))
                        || e.contains(t)
                    }
                });

                id_match && text_match
            });

            if found {
                Ok(())
            } else {
                let desc = target.and_then(|t| t.id.as_deref()).unwrap_or("(unnamed)");
                Err(format!("element not found: {desc}"))
            }
        }
        "element_state" => {
            let target = assert.target.as_ref().ok_or("assert element_state: no target")?;
            let elements = get_semantic_elements(dev)?;
            let id = target.id.as_deref().unwrap_or("");

            let elem = elements.iter().find(|e| {
                e.contains(&format!("platform_id: \"{}\"", id)) || e.contains(&format!("id: \"{}\"", id))
            });

            let elem = elem.ok_or_else(|| format!("element not found: {id}"))?;

            if let Some(expected_enabled) = assert.enabled {
                let is_clickable = elem.contains("clickable: true");
                if expected_enabled != is_clickable {
                    return Err(format!("expected enabled={expected_enabled}, got clickable={is_clickable}"));
                }
            }

            Ok(())
        }
        other => Err(format!("unknown assert: {other}")),
    }
}

fn find_element(dev: Option<&Device>, target: &Target) -> Result<(i32, i32), String> {
    // Try agent first for precise bounds
    let yaml = fetch_agent_yaml(dev)?;

    let search_id = target.id.as_deref().unwrap_or("");
    let search_text = target.text.as_deref().unwrap_or("");

    // Parse elements from YAML to find bounds
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
        let matches = id_match || text_match;

        if matches {
            // Extract center from bounds
            let x = extract_yaml_int(chunk, "x: ");
            let y = extract_yaml_int(chunk, "y: ");
            let w = extract_yaml_int(chunk, "w: ");
            let h = extract_yaml_int(chunk, "h: ");

            if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                // Convert dp back to px for input commands
                let density = get_density(dev).unwrap_or(2.8);
                let cx = ((x + w / 2) as f64 * density) as i32;
                let cy = ((y + h / 2) as f64 * density) as i32;
                return Ok((cx, cy));
            }
        }
    }

    Err(format!("element not found: id={search_id} text={search_text}"))
}

fn scroll_to_element(dev: Option<&Device>, id_or_text: &str) -> Result<(), String> {
    for _ in 0..10 {
        let yaml = fetch_agent_yaml(dev)?;
        if yaml.contains(id_or_text) {
            return Ok(());
        }
        scroll_direction(dev, "down")?;
        wait_idle(dev, 5);
    }
    Err(format!("could not scroll to: {id_or_text}"))
}

fn scroll_direction(dev: Option<&Device>, dir: &str) -> Result<(), String> {
    let (x1, y1, x2, y2) = match dir {
        "down" => (540, 1400, 540, 800),
        "up" => (540, 800, 540, 1400),
        "left" => (800, 1100, 200, 1100),
        "right" => (200, 1100, 800, 1100),
        _ => return Err(format!("unknown scroll direction: {dir}")),
    };
    adb::shell(dev, &[
        "input", "swipe",
        &x1.to_string(), &y1.to_string(),
        &x2.to_string(), &y2.to_string(),
        "500",
    ])?;
    Ok(())
}

fn get_current_activity(dev: Option<&Device>) -> Result<String, String> {
    let out = adb::shell(dev, &["dumpsys", "activity", "activities"])?;
    for line in out.lines() {
        if line.contains("mResumedActivity") || line.contains("topResumedActivity") {
            return Ok(line.trim().to_string());
        }
    }
    Ok(String::new())
}

fn wait_idle(dev: Option<&Device>, timeout: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    loop {
        if std::time::Instant::now() > deadline {
            break;
        }
        if let Ok(true) = check_idle(dev) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn check_idle(dev: Option<&Device>) -> Result<bool, String> {
    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "1", "http://localhost:9876/idle"])
        .output()
        .map_err(|e| format!("curl error: {e}"))?;

    let body = String::from_utf8_lossy(&resp.stdout);
    Ok(body.contains("\"idle\":true") || body.contains("\"idle\": true"))
}

fn fetch_agent_yaml(dev: Option<&Device>) -> Result<String, String> {
    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "2", "http://localhost:9876/semantic"])
        .output()
        .map_err(|e| format!("curl error: {e}"))?;

    if !resp.status.success() {
        return Err("agent not responding".to_string());
    }

    let body = String::from_utf8_lossy(&resp.stdout).into_owned();
    if body.contains("elements:") {
        Ok(body)
    } else {
        Err("invalid agent response".to_string())
    }
}

fn get_semantic_elements(dev: Option<&Device>) -> Result<Vec<String>, String> {
    let yaml = fetch_agent_yaml(dev)?;
    let elements: Vec<String> = yaml.split("\n- ")
        .skip(1)
        .map(|s| s.to_string())
        .collect();
    Ok(elements)
}

fn get_density(dev: Option<&Device>) -> Option<f64> {
    let out = adb::shell(dev, &["wm", "density"]).ok()?;
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("Physical density:") {
            return rest.trim().parse::<f64>().ok().map(|d| d / 160.0);
        }
    }
    None
}

fn extract_yaml_int(chunk: &str, key: &str) -> Option<i32> {
    for line in chunk.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn capture_failure_screenshot(dev: Option<&Device>, test_id: &str, step: usize) -> Option<String> {
    let path = format!("/tmp/ddb-test-fail-{}-step{}.png", test_id, step);
    let _ = adb::shell(dev, &["screencap", "-p", "/sdcard/fail.png"]);
    let _ = adb::adb(dev, &["pull", "/sdcard/fail.png", &path]);
    Some(path)
}
