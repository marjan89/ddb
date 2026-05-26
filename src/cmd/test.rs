use clap::Args;
use std::path::Path;

use crate::adb;
use crate::registry::{Device, Registry};

#[derive(serde::Serialize)]
struct RunLog {
    tc_id: String,
    tc_name: String,
    platform: String,
    device: String,
    started: String,
    finished: String,
    result: String,
    steps: Vec<StepLogEntry>,
}

#[derive(serde::Serialize)]
struct StepLogEntry {
    step: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assert: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    element_found: Option<String>,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_yaml: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ui_dump: Option<String>,
}

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
    #[serde(default)]
    pre_steps: Vec<StepRaw>,
    steps: Vec<StepRaw>,
    #[serde(default)]
    post_steps: Vec<StepRaw>,
}

struct TestSpec {
    id: String,
    name: String,
    precondition: Option<Precondition>,
    pre_steps: Vec<Step>,
    steps: Vec<Step>,
    post_steps: Vec<Step>,
}

#[derive(serde::Deserialize)]
struct Precondition {
    #[serde(default)]
    activity: Option<String>,
    #[serde(default)]
    scroll_to: Option<String>,
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    logged_in: Option<bool>,
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
    seconds: Option<u64>,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    times: Option<u64>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    site_id: Option<i64>,
    #[serde(default)]
    user_id: Option<i64>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    body: Option<serde_json::Value>,
    #[serde(default)]
    headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    save_as: Option<String>,
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
    seconds: Option<u64>,
    times: Option<u64>,
    url: Option<String>,
    site_id: Option<i64>,
    user_id: Option<i64>,
    method: Option<String>,
    body: Option<serde_json::Value>,
    headers: Option<std::collections::HashMap<String, String>>,
    save_as: Option<String>,
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
                seconds: self.seconds,
                times: self.times,
                url: self.url,
                site_id: self.site_id,
                user_id: self.user_id,
                method: self.method,
                body: self.body,
                headers: self.headers,
                save_as: self.save_as,
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
    #[serde(default)]
    content_fuzzy: Option<String>,
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
        let pre_steps: Vec<Step> = raw.pre_steps.into_iter()
            .map(|s| s.into_step())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid pre_step in {spec_path}: {e}"))?;
        let steps: Vec<Step> = raw.steps.into_iter()
            .map(|s| s.into_step())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid step in {spec_path}: {e}"))?;
        let post_steps: Vec<Step> = raw.post_steps.into_iter()
            .map(|s| s.into_step())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid post_step in {spec_path}: {e}"))?;
        let spec = TestSpec {
            id: raw.id,
            name: raw.name,
            precondition: raw.precondition,
            pre_steps,
            steps,
            post_steps,
        };

        let started = now_iso();
        let (result, step_logs) = run_spec(&spec, dev.as_ref(), args.step_timeout);
        let finished = now_iso();

        // Write structured run log
        let dev_name_str = dev.as_ref().map(|d| d.model.as_str()).unwrap_or("unknown");
        let run_log = RunLog {
            tc_id: spec.id.clone(),
            tc_name: spec.name.clone(),
            platform: "android".to_string(),
            device: dev_name_str.to_string(),
            started,
            finished: finished.clone(),
            result: result.status.clone(),
            steps: step_logs,
        };

        // Detect catalogue path from spec file path
        let results_dir = detect_results_dir(spec_path);
        if let Some(ref dir) = results_dir {
            let _ = std::fs::create_dir_all(dir);
            let ts_slug = now_timestamp_slug();
            let log_path = format!("{}/{}-android-{}.yaml", dir, spec.id, ts_slug);
            if let Ok(yaml_out) = serde_yaml::to_string(&run_log) {
                let _ = std::fs::write(&log_path, &yaml_out);
                eprintln!("run log → {log_path}");
            }
        }

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

fn detect_results_dir(spec_path: &str) -> Option<String> {
    let p = std::path::Path::new(spec_path);
    // Walk up looking for a "tests" directory — results go in tests/results/
    let mut dir = p.parent();
    while let Some(d) = dir {
        if d.file_name().map_or(false, |n| n == "tests") {
            return Some(d.join("results").to_string_lossy().to_string());
        }
        dir = d.parent();
    }
    // Fallback: put results next to the spec file
    p.parent().map(|d| d.join("results").to_string_lossy().to_string())
}

fn set_animations(enabled: bool) {
    let val = if enabled { "1" } else { "0" };
    let _ = std::process::Command::new("curl")
        .args(["-s", "-X", "POST", &format!("http://localhost:9876/animations?enabled={enabled}")])
        .output();
}

fn now_iso() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    let days = secs / 86400;
    let y = 1970 + days / 365;
    format!("{y}-01-01T{h:02}:{m:02}:{s:02}Z")
}

fn now_timestamp_slug() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_secs())
}

fn step_target_desc(step: &Step) -> Option<String> {
    match step {
        Step::Action(a) => a.target.as_ref().map(|t| {
            if let Some(ref id) = t.id { format!("{{id: \"{id}\"}}") }
            else if let Some(ref text) = t.text { format!("{{text: \"{text}\"}}") }
            else if let Some(ref fuzzy) = t.content_fuzzy { format!("{{content_fuzzy: \"{fuzzy}\"}}") }
            else { "{}".to_string() }
        }),
        Step::Assert(a) => a.target.as_ref().map(|t| {
            if let Some(ref id) = t.id { format!("{{id: \"{id}\"}}") }
            else if let Some(ref text) = t.text { format!("{{text: \"{text}\"}}") }
            else if let Some(ref fuzzy) = t.content_fuzzy { format!("{{content_fuzzy: \"{fuzzy}\"}}") }
            else { "{}".to_string() }
        }),
    }
}

fn ensure_input_focus(dev: Option<&Device>) {
    // Check if any input field is focused; if not, find and tap the first EditText
    if let Ok(out) = adb::shell(dev, &["dumpsys", "input_method"]) {
        if out.contains("mServedView=null") || !out.contains("mServedView=") {
            // No input field focused — find first EditText via agent
            if let Ok(yaml) = fetch_agent_yaml(dev) {
                for chunk in yaml.split("\n- ") {
                    if chunk.contains("type: input") || chunk.contains("type: text_field") || chunk.contains("EditText") {
                        let x = extract_yaml_int(chunk, "x: ");
                        let y = extract_yaml_int(chunk, "y: ");
                        let w = extract_yaml_int(chunk, "w: ");
                        let h = extract_yaml_int(chunk, "h: ");
                        if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                            let density = get_density(dev).unwrap_or(2.8);
                            let cx = ((x + w / 2) as f64 * density) as i32;
                            let cy = ((y + h / 2) as f64 * density) as i32;
                            let _ = adb::shell(dev, &["input", "tap", &cx.to_string(), &cy.to_string()]);
                            std::thread::sleep(std::time::Duration::from_millis(300));
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn dismiss_keyboard_if_visible(dev: Option<&Device>) {
    if let Ok(out) = adb::shell(dev, &["dumpsys", "input_method"]) {
        if out.contains("mInputShown=true") {
            let _ = adb::shell(dev, &["input", "keyevent", "4"]); // BACK dismisses keyboard
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
}

fn fetch_ui_dump(dev: Option<&Device>) -> String {
    match adb::shell(dev, &["uiautomator", "dump", "/dev/tty"]) {
        Ok(out) => out,
        Err(_) => "(ui dump failed)".to_string(),
    }
}

struct RunContext {
    vars: std::collections::HashMap<String, serde_json::Value>,
}

impl RunContext {
    fn new() -> Self {
        Self { vars: std::collections::HashMap::new() }
    }

    fn interpolate(&self, s: &str) -> String {
        let mut result = s.to_string();
        for (key, val) in &self.vars {
            let patterns = self.flatten_patterns(key, val);
            for (pattern, replacement) in patterns {
                result = result.replace(&format!("{{{{{}}}}}", pattern), &replacement);
            }
        }
        result
    }

    fn flatten_patterns(&self, prefix: &str, val: &serde_json::Value) -> Vec<(String, String)> {
        let mut out = Vec::new();
        match val {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    out.extend(self.flatten_patterns(&format!("{prefix}.{k}"), v));
                }
            }
            serde_json::Value::String(s) => out.push((prefix.to_string(), s.clone())),
            serde_json::Value::Number(n) => out.push((prefix.to_string(), n.to_string())),
            serde_json::Value::Bool(b) => out.push((prefix.to_string(), b.to_string())),
            _ => out.push((prefix.to_string(), val.to_string())),
        }
        out
    }

    fn save(&mut self, key: &str, val: serde_json::Value) {
        self.vars.insert(key.to_string(), val);
    }
}

fn run_spec(spec: &TestSpec, dev: Option<&Device>, timeout: u64) -> (TestResult, Vec<StepLogEntry>) {
    let mut step_logs: Vec<StepLogEntry> = Vec::new();
    let mut ctx = RunContext::new();

    // Always reset app to clean state before each TC
    let pkg = "se.naturkartan.android";
    let _ = adb::shell(dev, &["am", "force-stop", pkg]);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = adb::shell(dev, &[
        "am", "start", "-n",
        &format!("{pkg}/.ui.mainfragment.MainActivity"),
        "--activity-clear-task",
    ]);
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Check preconditions
    if let Some(ref pre) = spec.precondition {
        if let Some(ref activity) = pre.activity {
            if let Ok(current) = get_current_activity(dev) {
                if !current.contains(activity) {
                    return (TestResult {
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
                    }, step_logs);
                }
            }
        }
        if let Some(ref scroll_target) = pre.scroll_to {
            let _ = scroll_to_element(dev, scroll_target);
        }
    }

    wait_idle(dev, timeout);

    // Run pre_steps (API setup, data seeding)
    for (i, step) in spec.pre_steps.iter().enumerate() {
        let result = match step {
            Step::Action(a) => execute_action(dev, a, &mut ctx),
            Step::Assert(a) => execute_assert(dev, a, timeout),
        };
        if let Err(e) = result {
            eprintln!("  pre_step {} FAIL: {e}", i + 1);
            return (TestResult {
                id: spec.id.clone(),
                name: spec.name.clone(),
                status: "FAIL".to_string(),
                steps_run: 0,
                steps_total: spec.steps.len(),
                failure: Some(FailureDetail {
                    step: 0,
                    description: format!("pre_step {} failed: {e}", i + 1),
                    screenshot: None,
                }),
            }, step_logs);
        }
    }

    for (i, step) in spec.steps.iter().enumerate() {
        let result = match step {
            Step::Action(a) => execute_action(dev, a, &mut ctx),
            Step::Assert(a) => execute_assert(dev, a, timeout),
        };

        match &result {
            Ok(found_desc) => {
                let (action_name, assert_name) = match step {
                    Step::Action(a) => (Some(a.action.clone()), None),
                    Step::Assert(a) => (None, Some(a.assert.clone())),
                };
                // On PASS, include full ui dump as proof
                let ui_dump = if matches!(step, Step::Assert(_)) {
                    Some(fetch_ui_dump(dev))
                } else {
                    None
                };
                step_logs.push(StepLogEntry {
                    step: i + 1,
                    action: action_name,
                    assert: assert_name,
                    target: step_target_desc(step),
                    result: "PASS".to_string(),
                    element_found: if found_desc.is_empty() { None } else { Some(found_desc.clone()) },
                    timestamp: now_iso(),
                    error: None,
                    agent_yaml: None,
                    ui_dump,
                });
            }
            Err(err) => {
                let (action_name, assert_name) = match step {
                    Step::Action(a) => (Some(a.action.clone()), None),
                    Step::Assert(a) => (None, Some(a.assert.clone())),
                };
                let agent_yaml = fetch_agent_yaml(dev).ok();
                let ui_dump = Some(fetch_ui_dump(dev));
                let screenshot = capture_failure_screenshot(dev, &spec.id, i);

                step_logs.push(StepLogEntry {
                    step: i + 1,
                    action: action_name,
                    assert: assert_name,
                    target: step_target_desc(step),
                    result: "FAIL".to_string(),
                    element_found: None,
                    timestamp: now_iso(),
                    error: Some(err.clone()),
                    agent_yaml,
                    ui_dump,
                });

                return (TestResult {
                    id: spec.id.clone(),
                    name: spec.name.clone(),
                    status: "FAIL".to_string(),
                    steps_run: i,
                    steps_total: spec.steps.len(),
                    failure: Some(FailureDetail {
                        step: i + 1,
                        description: err.clone(),
                        screenshot,
                    }),
                }, step_logs);
            }
        }

        if matches!(step, Step::Action(_)) {
            wait_idle(dev, timeout);
        }
    }

    // On overall PASS, grab final ui dump as proof
    let final_ui = fetch_ui_dump(dev);
    if let Some(last) = step_logs.last_mut() {
        if last.ui_dump.is_none() {
            last.ui_dump = Some(final_ui);
        }
    }

    // Run post_steps (cleanup, API teardown) — always run, even conceptually on success
    for (i, step) in spec.post_steps.iter().enumerate() {
        let result = match step {
            Step::Action(a) => execute_action(dev, a, &mut ctx),
            Step::Assert(a) => execute_assert(dev, a, timeout),
        };
        if let Err(e) = &result {
            eprintln!("  post_step {} FAIL: {e}", i + 1);
        }
    }

    (TestResult {
        id: spec.id.clone(),
        name: spec.name.clone(),
        status: "PASS".to_string(),
        steps_run: spec.steps.len(),
        steps_total: spec.steps.len(),
        failure: None,
    }, step_logs)
}

fn execute_action(dev: Option<&Device>, action: &ActionStep, ctx: &mut RunContext) -> Result<String, String> {
    match action.action.as_str() {
        "tap" => {
            dismiss_keyboard_if_visible(dev);
            let target = action.target.as_ref().ok_or("tap: no target")?;
            let (x, y, desc) = poll_for_element(dev, target, 10_000)?;
            adb::shell(dev, &["input", "tap", &x.to_string(), &y.to_string()])?;
            Ok(desc)
        }
        "type" => {
            dismiss_keyboard_if_visible(dev);
            let text = action.text.as_ref().ok_or("type: no text")?;
            // Auto-focus: if a target is specified, tap it first; otherwise find focused field
            if let Some(ref target) = action.target {
                let (x, y, _) = find_element(dev, target)?;
                adb::shell(dev, &["input", "tap", &x.to_string(), &y.to_string()])?;
                std::thread::sleep(std::time::Duration::from_millis(300));
            } else {
                ensure_input_focus(dev);
            }
            let escaped = text.replace(' ', "%s");
            adb::shell(dev, &["input", "text", &escaped])?;
            Ok(format!("typed \"{}\"", text))
        }
        "scroll" | "scroll_to" => {
            if let Some(ref target) = action.target {
                let id_or_text = target.id.as_deref().or(target.text.as_deref()).unwrap_or("");
                scroll_to_element(dev, id_or_text)?;
            } else {
                let dir = action.direction.as_deref().unwrap_or("down");
                let times = action.times.unwrap_or(1);
                for _ in 0..times {
                    scroll_direction(dev, dir)?;
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            }
            Ok(String::new())
        }
        "deep_link" | "navigate_to_site" => {
            let site_id = action.site_id
                .map(|id| id.to_string())
                .or_else(|| {
                    action.url.as_deref()
                        .and_then(|u| u.rsplit('/').next())
                        .map(|s| s.to_string())
                })
                .ok_or("navigate_to_site: no site_id")?;
            adb::shell(dev, &[
                "am", "start", "-n",
                "se.naturkartan.android/.ui.sitedetail.SiteDetailActivity",
                "--ei", "extra_site_id", &site_id,
            ])?;
            std::thread::sleep(std::time::Duration::from_secs(3));
            Ok(format!("navigate_to_site → {site_id}"))
        }
        "navigate_to_user" => {
            let user_id = action.user_id
                .map(|id| id.to_string())
                .ok_or("navigate_to_user: no user_id")?;
            adb::shell(dev, &[
                "am", "start", "-n",
                "se.naturkartan.android/.ui.userprofile.UserProfileActivity",
                "--ei", "extra_user_id", &user_id,
            ])?;
            std::thread::sleep(std::time::Duration::from_secs(3));
            Ok(format!("navigate_to_user → {user_id}"))
        }
        "long_press" => {
            dismiss_keyboard_if_visible(dev);
            let target = action.target.as_ref().ok_or("long_press: no target")?;
            let (x, y, desc) = find_element(dev, target)?;
            adb::shell(dev, &["input", "swipe", &x.to_string(), &y.to_string(), &x.to_string(), &y.to_string(), "1500"])?;
            Ok(desc)
        }
        "back" => {
            adb::shell(dev, &["input", "keyevent", "4"])?;
            Ok(String::new())
        }
        "home" => {
            adb::shell(dev, &["input", "keyevent", "3"])?;
            Ok(String::new())
        }
        "wait" => {
            let secs = action.seconds.unwrap_or(2);
            std::thread::sleep(std::time::Duration::from_secs(secs));
            Ok(String::new())
        }
        "capture" => {
            let output_raw = action.output.as_ref().ok_or("capture: no output path")?;
            let output = output_raw.replace("{platform}", "android");
            let output_path = std::path::Path::new(&output);
            if let Some(parent) = output_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let cat_info = action
                .catalogue
                .as_deref()
                .map(|c| {
                    let key = crate::catalogue::detect_catalogue_path(&output).map(|(_, k)| k);
                    (std::path::PathBuf::from(c), key)
                })
                .or_else(|| {
                    crate::catalogue::detect_catalogue_path(&output)
                        .map(|(root, key)| (root, Some(key)))
                });

            let history_count = if output_path.exists() {
                crate::catalogue::archive_existing(output_path)?
            } else {
                0
            };

            let yaml = fetch_agent_yaml(dev)?;
            std::fs::write(&output, &yaml).map_err(|e| format!("write capture: {e}"))?;
            eprintln!("captured → {output}");

            if let Some((cat_root, Some(entry_key))) = cat_info {
                let schema: crate::semantic::SemanticSchema =
                    serde_yaml::from_str(&yaml)
                        .map_err(|e| format!("count elements: {e}"))?;
                let count = schema.elements.len() as u64;
                crate::catalogue::update_manifest_semantic(&cat_root, &entry_key, count, history_count)?;
            }
            Ok(format!("captured → {output}"))
        }
        "capture_screenshot" => {
            let output_raw = action.output.as_ref().ok_or("capture_screenshot: no output path")?;
            let output = output_raw.replace("{platform}", "android");
            let output_path = std::path::Path::new(&output);
            if let Some(parent) = output_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let cat_info = action
                .catalogue
                .as_deref()
                .map(|c| {
                    let key = crate::catalogue::detect_catalogue_path(&output).map(|(_, k)| k);
                    (std::path::PathBuf::from(c), key)
                })
                .or_else(|| {
                    crate::catalogue::detect_catalogue_path(&output)
                        .map(|(root, key)| (root, Some(key)))
                });

            if output_path.exists() {
                let _ = crate::catalogue::archive_existing(output_path);
            }

            let png = adb::adb_raw(dev, &["exec-out", "screencap", "-p"])?;
            std::fs::write(&output, &png).map_err(|e| format!("write screenshot: {e}"))?;
            let _ = std::process::Command::new("sips")
                .args(["-Z", "1200", &output])
                .output();
            eprintln!("screenshot → {output}");

            if let Some((cat_root, Some(entry_key))) = cat_info {
                let _ = crate::catalogue::update_manifest_screenshot(&cat_root, &entry_key);
            }
            Ok(format!("screenshot → {output}"))
        }
        "api_call" => {
            let raw_url = action.url.as_deref().ok_or("api_call: no url")?;
            let url = ctx.interpolate(raw_url);
            let method = action.method.as_deref().unwrap_or("GET");
            let base = "https://apiv3.naturkartan.se";
            let full_url = if url.starts_with("http") { url.to_string() } else { format!("{base}{url}") };

            let mut req = match method.to_uppercase().as_str() {
                "GET" => ureq::get(&full_url),
                "POST" => ureq::post(&full_url),
                "PUT" => ureq::put(&full_url),
                "DELETE" => ureq::delete(&full_url),
                "PATCH" => ureq::patch(&full_url),
                _ => return Err(format!("api_call: unsupported method {method}")),
            };

            if let Some(ref hdrs) = action.headers {
                for (k, v) in hdrs {
                    req = req.set(k, &ctx.interpolate(v));
                }
            }

            let resp = if let Some(ref body) = action.body {
                let body_str = ctx.interpolate(&body.to_string());
                req.set("Content-Type", "application/json")
                    .send_string(&body_str)
                    .map_err(|e| format!("api_call {method} {full_url}: {e}"))?
            } else {
                req.call().map_err(|e| format!("api_call {method} {full_url}: {e}"))?
            };

            let status = resp.status();
            let body_str = resp.into_string().unwrap_or_default();

            if status >= 400 {
                return Err(format!("api_call {method} {full_url}: HTTP {status} — {body_str}"));
            }

            if let Some(ref save_key) = action.save_as {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_str) {
                    ctx.save(save_key, json_val);
                }
            }

            Ok(format!("api_call {method} {full_url} → {status} ({} bytes)", body_str.len()))
        }
        other => Err(format!("unknown action: {other}")),
    }
}

fn execute_assert(dev: Option<&Device>, assert: &AssertStep, timeout: u64) -> Result<String, String> {
    match assert.assert.as_str() {
        "activity" => {
            let expected = assert.expected.as_ref().ok_or("assert activity: no expected")?;
            let current = get_current_activity(dev)?;
            if current.contains(expected.as_str()) {
                Ok(format!("activity matches: {current}"))
            } else {
                Err(format!("expected activity {expected}, got {current}"))
            }
        }
        "element_exists" => {
            let elements = get_semantic_elements(dev)?;
            let target = assert.target.as_ref();
            let expected_text = assert.text.as_deref();
            let expected_hint = assert.hint.as_deref();

            let found = elements.iter().find(|e| {
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

                let fuzzy_match = target
                    .and_then(|t| t.content_fuzzy.as_deref())
                    .map_or(true, |fuzzy| {
                        let lower = e.to_lowercase();
                        let needle = fuzzy.to_lowercase();
                        lower.contains(&needle) || e.lines().any(|line| {
                            let t = line.trim();
                            if let Some(rest) = t.strip_prefix("content:") {
                                token_jaccard(&needle, &rest.to_lowercase()) >= 0.6
                            } else {
                                false
                            }
                        })
                    });

                id_match && text_match && fuzzy_match
            });

            if let Some(elem) = found {
                let content_line = elem.lines()
                    .find(|l| l.trim().starts_with("content:"))
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default();
                Ok(format!("found: {content_line}"))
            } else {
                // Fallback: uiautomator dump catches AlertDialogs and system windows
                // that the semantic agent doesn't see
                let ui_xml = fetch_ui_dump(dev);
                let fuzzy = target.and_then(|t| t.content_fuzzy.as_deref());
                let id = target.and_then(|t| t.id.as_deref());
                let ui_lower = ui_xml.to_lowercase();

                let found_in_ui = fuzzy
                    .map(|f| ui_lower.contains(&f.to_lowercase()))
                    .unwrap_or(false)
                    || id
                        .map(|i| ui_xml.contains(&format!("resource-id=\"{}\"", i))
                            || ui_xml.contains(&format!(":id/{}\"", i)))
                        .unwrap_or(false)
                    || expected_text
                        .map(|t| ui_lower.contains(&t.to_lowercase()))
                        .unwrap_or(false);

                if found_in_ui {
                    let desc = fuzzy.or(id).unwrap_or("(unnamed)");
                    Ok(format!("found (uiautomator fallback): {desc}"))
                } else {
                    let desc = target
                        .and_then(|t| t.content_fuzzy.as_deref().or(t.id.as_deref()))
                        .unwrap_or("(unnamed)");
                    Err(format!("element not found: {desc}"))
                }
            }
        }
        "element_not_exists" => {
            let elements = get_semantic_elements(dev)?;
            let target = assert.target.as_ref();

            let found = elements.iter().find(|e| {
                let id_match = target
                    .and_then(|t| t.id.as_deref())
                    .map_or(true, |id| {
                        e.contains(&format!("platform_id: \"{}\"", id))
                        || e.contains(&format!("id: \"{}\"", id))
                    });

                let fuzzy_match = target
                    .and_then(|t| t.content_fuzzy.as_deref())
                    .map_or(true, |fuzzy| {
                        e.to_lowercase().contains(&fuzzy.to_lowercase())
                    });

                id_match && fuzzy_match
            });

            if found.is_none() {
                let desc = target
                    .and_then(|t| t.content_fuzzy.as_deref().or(t.id.as_deref()))
                    .unwrap_or("(unnamed)");
                Ok(format!("correctly absent: {desc}"))
            } else {
                let desc = target
                    .and_then(|t| t.content_fuzzy.as_deref().or(t.id.as_deref()))
                    .unwrap_or("(unnamed)");
                Err(format!("element should not exist but found: {desc}"))
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

            Ok(format!("element state OK: {id}"))
        }
        other => Err(format!("unknown assert: {other}")),
    }
}

fn find_element(dev: Option<&Device>, target: &Target) -> Result<(i32, i32, String), String> {
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
                if let Some(rest) = t.strip_prefix("content:") {
                    let hay = rest.to_lowercase();
                    hay.contains(&needle) || token_jaccard(&needle, &hay) >= 0.6
                } else {
                    false
                }
            })
        };

        let exact_match = id_match || text_match;

        if exact_match || fuzzy_match {
            let x = extract_yaml_int(chunk, "x: ");
            let y = extract_yaml_int(chunk, "y: ");
            let w = extract_yaml_int(chunk, "w: ");
            let h = extract_yaml_int(chunk, "h: ");

            if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                let density = get_density(dev).unwrap_or(2.8);
                let cx = ((x + w / 2) as f64 * density) as i32;
                let cy = ((y + h / 2) as f64 * density) as i32;

                let content_line = chunk.lines()
                    .find(|l| l.trim().starts_with("content:"))
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default();
                let desc = format!("{} at ({}, {})", content_line, cx, cy);

                if exact_match {
                    return Ok((cx, cy, desc));
                }
                let is_clickable = chunk.contains("clickable: true");
                match (is_clickable, fuzzy_clickable) {
                    (true, false) => { fuzzy_candidate = Some((cx, cy, desc)); fuzzy_clickable = true; }
                    (true, true) => {}
                    (_, _) if fuzzy_candidate.is_none() => { fuzzy_candidate = Some((cx, cy, desc)); }
                    _ => {}
                }
            }
        }
    }

    if let Some(candidate) = fuzzy_candidate {
        return Ok(candidate);
    }

    let desc = if !search_id.is_empty() { search_id }
        else if !search_text.is_empty() { search_text }
        else { search_fuzzy };
    Err(format!("element not found: {desc}"))
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

fn token_jaccard(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let a_tokens: HashSet<&str> = a.split_whitespace().collect();
    let b_tokens: HashSet<&str> = b.split_whitespace().collect();
    let intersection = a_tokens.intersection(&b_tokens).count() as f64;
    let union = a_tokens.union(&b_tokens).count() as f64;
    if union == 0.0 { return 0.0; }
    intersection / union
}

fn poll_for_element(dev: Option<&Device>, target: &Target, timeout_ms: u64) -> Result<(i32, i32, String), String> {
    let start = std::time::Instant::now();
    let interval = std::time::Duration::from_millis(500);
    let timeout = std::time::Duration::from_millis(timeout_ms);

    wait_idle(dev, 5);

    loop {
        match find_element(dev, target) {
            Ok(result) => return Ok(result),
            Err(_) if start.elapsed() < timeout => {
                std::thread::sleep(interval);
            }
            Err(e) => return Err(e),
        }
    }
}

fn capture_failure_screenshot(dev: Option<&Device>, test_id: &str, step: usize) -> Option<String> {
    let path = format!("/tmp/ddb-test-fail-{}-step{}.png", test_id, step);
    let _ = adb::shell(dev, &["screencap", "-p", "/sdcard/fail.png"]);
    let _ = adb::adb(dev, &["pull", "/sdcard/fail.png", &path]);
    Some(path)
}
