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

    /// Re-run only failed TCs from the matrix
    #[arg(long)]
    pub rerun_failed: bool,

    /// Run TCs in order from a suite YAML file
    #[arg(long)]
    pub suite: Option<String>,

    /// Results directory for matrix lookup
    #[arg(long, env = "DDB_RESULTS_DIR")]
    pub results_dir: Option<String>,

    /// Test cases directory
    #[arg(long, env = "DDB_TESTS_DIR")]
    pub tests_dir: Option<String>,

    /// Expected agent git hash (error if mismatch)
    #[arg(long, env = "DDB_EXPECTED_HASH")]
    pub expected_hash: Option<String>,

    /// Build and install APK before running TCs
    #[arg(long)]
    pub build: bool,

    /// Project directory for --build (default: DDB_PROJECT_DIR env var)
    #[arg(long, env = "DDB_PROJECT_DIR")]
    pub project_dir: Option<String>,
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
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    logged_in: Option<bool>,
}

#[derive(serde::Deserialize, Clone)]
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
    platform: Option<PlatformSteps>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    body: Option<serde_json::Value>,
    #[serde(default)]
    headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    save_as: Option<String>,
}

#[derive(serde::Deserialize, Clone)]
struct PlatformSteps {
    #[serde(default)]
    android: Option<Vec<StepRaw>>,
    #[serde(default)]
    ios: Option<Vec<StepRaw>>,
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

#[derive(serde::Deserialize, Clone)]
struct Target {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content_fuzzy: Option<String>,
    #[serde(default)]
    clickable_only: Option<bool>,
    #[serde(default)]
    exclude_type: Option<String>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
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

    // Build + install if --build flag
    if args.build {
        let project_dir = args.project_dir.as_deref()
            .ok_or("--build requires --project-dir or DDB_PROJECT_DIR")?;
        eprintln!("building APK from {project_dir}...");
        let build_status = std::process::Command::new("nosandbox")
            .args(&[
                "./gradlew",
                &std::env::var("DDB_BUILD_TASK").unwrap_or_else(|_| "assembleStandardDebug".into()),
                "--no-daemon",
            ])
            .current_dir(project_dir)
            .status()
            .map_err(|e| format!("build failed: {e}"))?;
        if !build_status.success() {
            return Err("APK build failed".into());
        }
        let apk_src = std::env::var("DDB_APK_SRC").unwrap_or_else(|_|
            format!("{project_dir}/app/build/outputs/apk/standard/debug/app-standard-debug.apk"));
        let apk_dst = std::env::var("DDB_APK_PATH").unwrap_or_else(|_| "/tmp/app-debug.apk".into());
        std::fs::copy(&apk_src, &apk_dst)
            .map_err(|e| format!("copy APK: {e}"))?;
        eprintln!("installing APK...");
        let install_result = adb::adb(dev.as_ref(), &["install", "-r", &apk_dst]);
        if install_result.is_err() {
            return Err("APK install failed".into());
        }
        eprintln!("APK installed. waiting for app launch...");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }

    // Version check: verify agent is running expected build
    if let Some(ref expected) = args.expected_hash {
        let base = agent_base_url();
        let out = std::process::Command::new("curl")
            .args(["-s", "--max-time", "5", &format!("{base}/version")])
            .output();
        if let Ok(out) = out {
            let body = String::from_utf8_lossy(&out.stdout);
            if let Some(hash_start) = body.find("\"git_hash\":\"") {
                let rest = &body[hash_start + 12..];
                if let Some(end) = rest.find('"') {
                    let installed = &rest[..end];
                    if installed != expected.as_str() {
                        return Err(format!("STALE BINARY: installed={installed} expected={expected}. Rebuild APK."));
                    }
                    eprintln!("  agent version: {installed} ✓");
                }
            }
        }
    }

    // Resolve specs: suite > rerun-failed > explicit list
    let specs = if let Some(ref suite_path) = args.suite {
        let suite_content = std::fs::read_to_string(suite_path)
            .map_err(|e| format!("read suite {suite_path}: {e}"))?;
        let suite_dir = std::path::Path::new(suite_path).parent().unwrap_or(std::path::Path::new("."));
        let mut ordered = Vec::new();
        for line in suite_content.lines() {
            let trimmed = line.trim().trim_start_matches('-').trim();
            if trimmed.ends_with(".yaml") && !trimmed.starts_with('#') {
                let tc_path = suite_dir.join(trimmed);
                if tc_path.exists() {
                    ordered.push(tc_path.to_str().unwrap_or("").to_string());
                } else {
                    eprintln!("  suite: skipping {} (not found)", trimmed);
                }
            }
        }
        println!("Suite: {} TCs from {}", ordered.len(), suite_path);
        ordered
    } else if args.rerun_failed {
        let results_dir = args.results_dir.as_deref().ok_or("--rerun-failed requires --results-dir or DDB_RESULTS_DIR")?;
        let tests_dir = args.tests_dir.as_deref().ok_or("--rerun-failed requires --tests-dir or DDB_TESTS_DIR")?;
        let failed = get_failed_tc_specs(results_dir, tests_dir)?;
        if failed.is_empty() {
            println!("All TCs passing — nothing to rerun.");
            return Ok(());
        }
        println!("Re-running {} failed TCs:", failed.len());
        for s in &failed {
            println!("  {}", s);
        }
        failed
    } else {
        args.specs.clone()
    };

    if specs.is_empty() {
        return Err("no test spec files provided".to_string());
    }

    // Set up port forwarding for agent (DDB_AGENT_PORT overrides local port)
    let agent_port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    if let Some(ref d) = dev {
        let _ = adb::adb(Some(d), &["forward", &format!("tcp:{agent_port}"), "tcp:9876"]);
    }
    if agent_port != "9876" {
        eprintln!("  agent port: {agent_port} (forwarded to device 9876)");
    }

    // Disable animations for reliable test execution
    set_animations(false);

    let mut results = Vec::new();
    let mut pass = 0;
    let mut fail = 0;

    for spec_path in &specs {
        let content = std::fs::read_to_string(spec_path)
            .map_err(|e| format!("read {spec_path}: {e}"))?;
        let raw: TestSpecRaw = serde_yaml::from_str(&content)
            .map_err(|e| format!("parse {spec_path}: {e}"))?;
        let expanded: Vec<StepRaw> = raw.steps.into_iter()
            .flat_map(|s| {
                let action_name = s.action.as_deref().unwrap_or("");
                if action_name == "navigate_to_site" || action_name == "navigate_to_user" {
                    return vec![s];
                }
                if let Some(ref plat) = s.platform {
                    if let Some(android_steps) = &plat.android {
                        return android_steps.clone();
                    }
                }
                vec![s]
            })
            .collect();
        let steps: Vec<Step> = expanded.into_iter()
            .map(|s| s.into_step())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid step in {spec_path}: {e}"))?;
        let spec = TestSpec {
            id: raw.id,
            name: raw.name,
            precondition: raw.precondition,
            steps,
        };

        switchboard_notify(&format!("run started {} — {}", spec.id, spec.name));
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
            switchboard_notify(&format!("run finished {} PASS", result.id));
        } else {
            fail += 1;
            let empty = String::new();
            let detail = result.failure.as_ref().map(|f| &f.description).unwrap_or(&empty);
            println!("  FAIL  {} — {} (step {}: {})",
                result.id, result.name,
                result.failure.as_ref().map(|f| f.step).unwrap_or(0),
                detail
            );
            switchboard_notify(&format!("run finished {} FAIL step {}: {}",
                result.id, result.failure.as_ref().map(|f| f.step).unwrap_or(0), detail));
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

fn agent_base_url() -> String {
    let port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| "9876".into());
    format!("http://localhost:{port}")
}

fn set_animations(enabled: bool) {
    let _ = std::process::Command::new("curl")
        .args(["-s", "--max-time", "3", "-X", "POST", &format!("{}/animations?enabled={enabled}", agent_base_url())])
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
                            let cx = x + w / 2;
                            let cy = y + h / 2;
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

fn get_failed_tc_specs(results_dir: &str, tests_dir: &str) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("vdb")
        .args(["matrix", "--results", results_dir, "--json"])
        .output()
        .map_err(|e| format!("vdb matrix: {e}"))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|e| format!("parse matrix json: {e}"))?;

    let mut specs = Vec::new();
    let tests_path = std::path::Path::new(tests_dir);

    for entry in &entries {
        let platform = entry["platform"].as_str().unwrap_or("");
        let result = entry["result"].as_str().unwrap_or("");
        let tc_id = entry["tc_id"].as_str().unwrap_or("");

        if platform != "android" || result == "PASS" || tc_id.is_empty() {
            continue;
        }

        // Find the TC YAML file — try common naming patterns
        let candidates = [
            format!("qa-{}.yaml", tc_id.to_lowercase().replace("tc-", "")),
            format!("{}.yaml", tc_id.to_lowercase()),
        ];
        for candidate in &candidates {
            let path = tests_path.join(candidate);
            if path.exists() {
                specs.push(path.to_str().unwrap_or("").to_string());
                break;
            }
        }
        // Also try glob: any file containing the TC ID
        if let Ok(entries) = std::fs::read_dir(tests_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_str().unwrap_or("").to_string();
                if fname.ends_with(".yaml") && !fname.contains("suite") && !fname.contains("results") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if content.contains(&format!("id: {}", tc_id)) {
                            let p = entry.path().to_str().unwrap_or("").to_string();
                            if !specs.contains(&p) {
                                specs.push(p);
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    specs.sort();
    specs.dedup();
    Ok(specs)
}

fn ensure_logged_in(dev: Option<&Device>, _pkg: &str) {
    let base = agent_base_url();

    // Check auth state via agent
    let state_out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "5", &format!("{base}/auth/state")])
        .output();
    if let Ok(out) = state_out {
        let body = String::from_utf8_lossy(&out.stdout);
        if body.contains("\"logged_in\":true") {
            eprintln!("  already logged in");
            return;
        }
    }

    let email = match std::env::var("DDB_TEST_EMAIL") {
        Ok(e) => e,
        Err(_) => { eprintln!("  ERROR: DDB_TEST_EMAIL not set — cannot login"); return; }
    };
    let password = match std::env::var("DDB_TEST_PASSWORD") {
        Ok(p) => p,
        Err(_) => { eprintln!("  ERROR: DDB_TEST_PASSWORD not set — cannot login"); return; }
    };
    eprintln!("  logging in as {} via agent...", email);

    let payload = format!(r#"{{"email":"{}","password":"{}"}}"#,
        email.replace('"', "\\\""), password.replace('"', "\\\""));
    let login_out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "10", "-X", "POST",
               "-H", "Content-Type: application/json",
               "-d", &payload,
               &format!("{base}/auth/login")])
        .output();
    match login_out {
        Ok(out) => {
            let body = String::from_utf8_lossy(&out.stdout);
            if body.contains("\"logged_in\":true") {
                eprintln!("  login complete");
            } else {
                eprintln!("  login failed: {}", body.trim());
            }
        }
        Err(e) => eprintln!("  login curl failed: {e}"),
    }
}

fn grant_all_permissions(dev: Option<&Device>, pkg: &str) {
    if let Ok(dump) = adb::shell(dev, &["pm", "dump", pkg]) {
        for line in dump.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("android.permission.") && trimmed.contains(": granted=false") {
                let perm = trimmed.split(':').next().unwrap_or("").trim();
                if !perm.is_empty() {
                    let _ = adb::shell(dev, &["pm", "grant", pkg, perm]);
                }
            }
        }
    }
    // Fallback: always try the common runtime permissions
    for perm in &[
        "android.permission.ACCESS_FINE_LOCATION",
        "android.permission.ACCESS_COARSE_LOCATION",
        "android.permission.POST_NOTIFICATIONS",
    ] {
        let _ = adb::shell(dev, &["pm", "grant", pkg, perm]);
    }
}

fn dismiss_permission_dialog(dev: Option<&Device>) {
    let ui = fetch_ui_dump(dev);
    let ui_lower = ui.to_lowercase();
    // Check for permission dialog keywords
    if ui_lower.contains("permission") || ui_lower.contains("allow") || ui_lower.contains("while using") {
        // Try common permission button IDs
        let perm_buttons = std::env::var("DDB_PERMISSION_BUTTONS")
            .unwrap_or_else(|_| "permission_allow_foreground_only_button,permission_allow_button".into());
        for btn_id in perm_buttons.split(',') {
            let btn_id = btn_id.trim();
            if ui.contains(btn_id) {
                let _ = adb::shell(dev, &[
                    "input", "keyevent", "KEYCODE_TAB",
                ]);
                // Find and tap the button via uiautomator coordinates
                if let Some(bounds) = extract_ui_bounds(&ui, btn_id) {
                    let _ = adb::shell(dev, &["input", "tap", &bounds.0.to_string(), &bounds.1.to_string()]);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    return;
                }
            }
        }
        // Fallback: tap "While using the app" text
        if let Some(bounds) = extract_ui_text_bounds(&ui, "While using") {
            let _ = adb::shell(dev, &["input", "tap", &bounds.0.to_string(), &bounds.1.to_string()]);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

fn extract_ui_bounds(xml: &str, resource_id: &str) -> Option<(i32, i32)> {
    let id_pattern = format!("id/{}", resource_id);
    if let Some(idx) = xml.find(&id_pattern) {
        let chunk = &xml[idx..xml.len().min(idx + 200)];
        if let Some(b_start) = chunk.find("bounds=\"[") {
            let bounds_str = &chunk[b_start + 9..];
            if let Some(b_end) = bounds_str.find(']') {
                let coords: Vec<&str> = bounds_str[..b_end].split(',').collect();
                if coords.len() == 2 {
                    let x1: i32 = coords[0].parse().unwrap_or(0);
                    let y1: i32 = coords[1].parse().unwrap_or(0);
                    // Get second bracket for x2,y2
                    if let Some(b2) = bounds_str[b_end+2..].find(']') {
                        let c2: Vec<&str> = bounds_str[b_end+2..b_end+2+b2].split(',').collect();
                        if c2.len() == 2 {
                            let x2: i32 = c2[0].parse().unwrap_or(0);
                            let y2: i32 = c2[1].parse().unwrap_or(0);
                            return Some(((x1 + x2) / 2, (y1 + y2) / 2));
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_ui_text_bounds(xml: &str, text: &str) -> Option<(i32, i32)> {
    let pattern = format!("text=\"{}", text);
    if let Some(idx) = xml.find(&pattern) {
        let chunk = &xml[idx..xml.len().min(idx + 300)];
        return extract_ui_bounds(chunk, "");
    }
    None
}

fn extract_ui_bounds_fuzzy(xml: &str, fuzzy: &str) -> Option<(i32, i32)> {
    let lower = xml.to_lowercase();
    let needle = fuzzy.to_lowercase();
    if let Some(idx) = lower.find(&needle) {
        let chunk = &xml[idx..xml.len().min(idx + 300)];
        if let Some(b_start) = chunk.find("bounds=\"[") {
            let bounds_str = &chunk[b_start + 9..];
            if let Some(b_end) = bounds_str.find(']') {
                let coords: Vec<&str> = bounds_str[..b_end].split(',').collect();
                if coords.len() == 2 {
                    let x1: i32 = coords[0].parse().unwrap_or(0);
                    let y1: i32 = coords[1].parse().unwrap_or(0);
                    if let Some(b2) = bounds_str[b_end+2..].find(']') {
                        let c2: Vec<&str> = bounds_str[b_end+2..b_end+2+b2].split(',').collect();
                        if c2.len() == 2 {
                            let x2: i32 = c2[0].parse().unwrap_or(0);
                            let y2: i32 = c2[1].parse().unwrap_or(0);
                            return Some(((x1 + x2) / 2, (y1 + y2) / 2));
                        }
                    }
                }
            }
        }
        // bounds may be before the match — search backwards
        let before = &xml[..idx + needle.len()];
        if let Some(node_start) = before.rfind('<') {
            let node = &xml[node_start..xml.len().min(idx + 500)];
            if let Some(b_start) = node.find("bounds=\"[") {
                let bounds_str = &node[b_start + 9..];
                if let Some(b_end) = bounds_str.find(']') {
                    let coords: Vec<&str> = bounds_str[..b_end].split(',').collect();
                    if coords.len() == 2 {
                        let x1: i32 = coords[0].parse().unwrap_or(0);
                        let y1: i32 = coords[1].parse().unwrap_or(0);
                        if let Some(b2) = bounds_str[b_end+2..].find(']') {
                            let c2: Vec<&str> = bounds_str[b_end+2..b_end+2+b2].split(',').collect();
                            if c2.len() == 2 {
                                let x2: i32 = c2[0].parse().unwrap_or(0);
                                let y2: i32 = c2[1].parse().unwrap_or(0);
                                return Some(((x1 + x2) / 2, (y1 + y2) / 2));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn dismiss_keyboard_if_visible(dev: Option<&Device>) {
    if let Ok(out) = adb::shell(dev, &["dumpsys", "input_method"]) {
        if out.contains("mInputShown=true") {
            let _ = adb::shell(dev, &["input", "keyevent", "111"]); // KEYCODE_ESCAPE dismisses keyboard without BACK navigation
            std::thread::sleep(std::time::Duration::from_secs(1));
            wait_idle(dev, 3);
        }
    }
}

fn fetch_ui_dump(dev: Option<&Device>) -> String {
    let _ = adb::shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]);
    match adb::shell(dev, &["cat", "/sdcard/ui.xml"]) {
        Ok(out) => out,
        Err(_) => "(ui dump failed)".to_string(),
    }
}

struct RunContext {
    vars: std::collections::HashMap<String, serde_json::Value>,
}

impl RunContext {
    fn new() -> Self { Self { vars: std::collections::HashMap::new() } }

    fn interpolate(&self, s: &str) -> String {
        let mut result = s.to_string();
        for (key, val) in &self.vars {
            self.apply_patterns(&mut result, key, val);
        }
        result
    }

    fn apply_patterns(&self, result: &mut String, prefix: &str, val: &serde_json::Value) {
        match val {
            serde_json::Value::Object(map) => {
                for (k, v) in map { self.apply_patterns(result, &format!("{prefix}.{k}"), v); }
            }
            serde_json::Value::String(s) => { *result = result.replace(&format!("{{{{{prefix}}}}}"), s); }
            serde_json::Value::Number(n) => { *result = result.replace(&format!("{{{{{prefix}}}}}"), &n.to_string()); }
            _ => {}
        }
    }
}

fn run_spec(spec: &TestSpec, dev: Option<&Device>, timeout: u64) -> (TestResult, Vec<StepLogEntry>) {
    let mut step_logs: Vec<StepLogEntry> = Vec::new();
    let mut ctx = RunContext::new();

    // Reset to MainActivity: use monkey (same as ddb app launch) after clearing task
    let pkg_env = std::env::var("DDB_TEST_PACKAGE").ok();
    let pkg = spec.precondition.as_ref()
        .and_then(|p| p.package.as_deref())
        .or(pkg_env.as_deref())
        .expect("No package name. Set DDB_TEST_PACKAGE env var or add precondition.package to TC YAML.");
    let main_activity = std::env::var("DDB_MAIN_ACTIVITY").unwrap_or_else(|_| format!("{pkg}/.ui.MainActivity"));
    let _ = adb::shell(dev, &[
        "am", "start",
        "-a", "android.intent.action.MAIN",
        "-c", "android.intent.category.LAUNCHER",
        "-n", &main_activity,
        "--activity-clear-task",
    ]);
    std::thread::sleep(std::time::Duration::from_secs(3));
    wait_idle(dev, 10);

    // Verify app is foreground after relaunch
    if let Ok(current) = get_current_activity(dev) {
        if !current.contains(pkg) {
            eprintln!("  warning: app not foreground ({}), retrying launch...", current.trim());
            let _ = adb::shell(dev, &[
                "am", "start", "-a", "android.intent.action.MAIN",
                "-c", "android.intent.category.LAUNCHER",
                "-n", &main_activity, "--activity-clear-task",
            ]);
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    }

    // Grant all app permissions from manifest (prevents any dialog)
    grant_all_permissions(dev, pkg);

    // Agent handshake: verify semantic agent identity
    let base = agent_base_url();
    let health_out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "5", &format!("{base}/health")])
        .output();
    match health_out {
        Ok(out) => {
            let body = String::from_utf8_lossy(&out.stdout);
            if body.contains("semantic-agent") {
                eprintln!("  agent: OK");
            } else {
                eprintln!("  warning: agent not responding, waiting 5s...");
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
        Err(_) => {
            eprintln!("  warning: agent unreachable, waiting 5s...");
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }

    // Handle logged_in precondition
    if let Some(ref pre) = spec.precondition {
        if pre.logged_in == Some(true) {
            ensure_logged_in(dev, pkg);
        }
    }
    if let Some(ref pre) = spec.precondition {
        if pre.logged_in == Some(false) {
            let _ = adb::shell(dev, &["pm", "clear", pkg]);
            std::thread::sleep(std::time::Duration::from_secs(1));
            let _ = adb::shell(dev, &[
                "am", "start",
                "-a", "android.intent.action.MAIN",
                "-c", "android.intent.category.LAUNCHER",
                "-n", &main_activity,
            ]);
            std::thread::sleep(std::time::Duration::from_secs(5));
            grant_all_permissions(dev, pkg);
            wait_idle(dev, 10);
        }
    }

    // Check preconditions (with retry + permission auto-dismiss)
    if let Some(ref pre) = spec.precondition {
        if let Some(ref activity) = pre.activity {
            let mut precondition_ok = false;
            for retry in 0..3 {
                if let Ok(current) = get_current_activity(dev) {
                    if current.contains(activity) {
                        precondition_ok = true;
                        break;
                    }
                    if current.contains("GrantPermissions") || current.contains("Permission") {
                        dismiss_permission_dialog(dev);
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                    if retry == 2 {
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
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
        if let Some(ref scroll_target) = pre.scroll_to {
            let _ = scroll_to_element(dev, scroll_target);
        }
    }

    wait_idle(dev, timeout);

    // Load fixtures from navigation.yaml if present
    let nav_env = std::env::var("DDB_NAVIGATION_YAML").ok();
    let nav_path_str = nav_env.as_deref().unwrap_or("catalogue/android/navigation.yaml");
    let nav_path = std::path::Path::new(nav_path_str);
    if nav_path.exists() {
        if let Ok(nav_content) = std::fs::read_to_string(nav_path) {
            // Extract fixture values: sites.*.name, users.*.email, etc.
            let mut in_fixtures = false;
            let mut current_key = String::new();
            for line in nav_content.lines() {
                let trimmed = line.trim();
                if trimmed == "fixtures:" { in_fixtures = true; continue; }
                if !in_fixtures { continue; }
                if !line.starts_with(' ') && !trimmed.is_empty() { break; }
                if trimmed.ends_with(':') && !trimmed.contains('{') {
                    current_key = trimmed.trim_end_matches(':').to_string();
                } else if trimmed.contains(": ") && !current_key.is_empty() {
                    let parts: Vec<&str> = trimmed.splitn(2, ": ").collect();
                    if parts.len() == 2 {
                        let k = format!("fixture.{}.{}", current_key, parts[0].trim());
                        let v = parts[1].trim().trim_matches('"').trim_matches('\'');
                        ctx.vars.insert(k, serde_json::Value::String(v.to_string()));
                    }
                }
            }
        }
    }

    // TC YAML validation: reject steps with missing targets
    for (i, step) in spec.steps.iter().enumerate() {
        match step {
            Step::Action(a) if a.action == "tap" || a.action == "long_press" || a.action == "scroll_to" => {
                if a.target.is_none() {
                    return (TestResult {
                        id: spec.id.clone(), name: spec.name.clone(),
                        status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
                        failure: Some(FailureDetail {
                            step: i + 1,
                            description: format!("YAML lint: {} action has no target", a.action),
                            screenshot: None,
                        }),
                    }, step_logs);
                }
            }
            _ => {}
        }
    }

    // Load standalone fixtures.yaml if present
    let fixtures_paths = ["catalogue/fixtures.yaml", "catalogue/tests/fixtures.yaml"];
    for fp in &fixtures_paths {
        let fp = std::path::Path::new(fp);
        if fp.exists() {
            if let Ok(content) = std::fs::read_to_string(fp) {
                if let Ok(val) = serde_yaml::from_str::<serde_json::Value>(&content) {
                    ctx.vars.insert("fixtures".into(), val);
                }
            }
            break;
        }
    }

    if let Some(serde_json::Value::String(v)) = ctx.vars.get("config.deprioritize_patterns") {
        if std::env::var("DDB_DEPRIORITIZE_PATTERNS").is_err() {
            unsafe { std::env::set_var("DDB_DEPRIORITIZE_PATTERNS", v); }
        }
    }
    if let Some(serde_json::Value::String(v)) = ctx.vars.get("config.jaccard_threshold") {
        if std::env::var("DDB_JACCARD_THRESHOLD").is_err() {
            unsafe { std::env::set_var("DDB_JACCARD_THRESHOLD", v); }
        }
    }

    let tc_deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout * spec.steps.len() as u64);

    for (i, step) in spec.steps.iter().enumerate() {
        // TC-level timeout
        if std::time::Instant::now() > tc_deadline {
            return (TestResult {
                id: spec.id.clone(), name: spec.name.clone(),
                status: "FAIL".to_string(), steps_run: i, steps_total: spec.steps.len(),
                failure: Some(FailureDetail {
                    step: i + 1, description: "TC timeout exceeded".to_string(), screenshot: None,
                }),
            }, step_logs);
        }

        let result = match step {
            Step::Action(a) => execute_action(dev, a, &mut ctx),
            Step::Assert(a) => execute_assert(dev, a, timeout),
        };
        // Retry once on failure (handles async content, transient UI state)
        let result = if result.is_err() {
            eprintln!("  step {} failed, retrying in 2s...", i + 1);
            std::thread::sleep(std::time::Duration::from_secs(2));
            match step {
                Step::Action(a) => execute_action(dev, a, &mut ctx),
                Step::Assert(a) => execute_assert(dev, a, timeout),
            }
        } else {
            result
        };

        match &result {
            Ok(found_desc) => {
                let step_desc = match step {
                    Step::Action(a) => format!("{}", a.action),
                    Step::Assert(a) => format!("assert {}", a.assert),
                };
                eprintln!("  step {}/{}: {} ✓", i + 1, spec.steps.len(), step_desc);
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
                let debug_log = fetch_debug_log();
                if let Some(ref log) = debug_log {
                    eprintln!("  debug-log: {}", &log[..log.len().min(200)]);
                }
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
            adb::shell(dev, &["input", "swipe", &x.to_string(), &y.to_string(), &x.to_string(), &y.to_string(), "50"])?;
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
            // Clear existing field content: Ctrl+A (select all) + Delete
            adb::shell(dev, &["input", "keyevent", "KEYCODE_MOVE_HOME"])?;
            adb::shell(dev, &["input", "keyevent", "--longpress", "KEYCODE_SHIFT_LEFT", "KEYCODE_MOVE_END"])?;
            adb::shell(dev, &["input", "keyevent", "KEYCODE_DEL"])?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            let has_non_ascii = text.chars().any(|c| !c.is_ascii());
            if has_non_ascii {
                // Clipboard paste for non-ASCII text (ö, å, ä, etc.)
                adb::shell(dev, &["am", "broadcast", "-a", "clipper.set", "-e", "text", text])?;
                std::thread::sleep(std::time::Duration::from_millis(200));
                // Long press to trigger paste menu
                let (x, y) = if let Some(ref target) = action.target {
                    let (x, y, _) = find_element(dev, target)?;
                    (x, y)
                } else {
                    (540, 300) // fallback center-ish
                };
                adb::shell(dev, &["input", "swipe", &x.to_string(), &y.to_string(), &x.to_string(), &y.to_string(), "1500"])?;
                std::thread::sleep(std::time::Duration::from_millis(500));
                // Tap paste button
                adb::shell(dev, &["input", "keyevent", "279"])?; // KEYCODE_PASTE
                std::thread::sleep(std::time::Duration::from_millis(300));
            } else {
                let escaped = text.replace(' ', "%s");
                adb::shell(dev, &["input", "text", &escaped])?;
            }
            Ok(format!("typed \"{}\"", text))
        }
        "scroll" | "scroll_to" => {
            if let Some(ref target) = action.target {
                // Page-stable preflight: informational only (async content may not be in dump yet)
                // Scroll until element is in viewport
                for attempt in 0..20 {
                    if find_element(dev, target).is_ok() {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        break;
                    }
                    if attempt == 19 {
                        return Err(format!("scroll_to: element not found in viewport after 20 scrolls"));
                    }
                    scroll_direction(dev, "down")?;
                    wait_idle(dev, 3);
                }
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
        "navigate_to_site" => {
            let site_id = action.site_id
                .map(|id| id.to_string())
                .or_else(|| ctx.vars.get("site_id").and_then(|v| v.as_str().map(|s| s.to_string())).or_else(|| ctx.vars.get("site_id").and_then(|v| v.as_i64().map(|n| n.to_string()))))
                .ok_or("navigate_to_site: no site_id")?;
            let base = agent_base_url();
            let out = std::process::Command::new("curl")
                .args(["-s", "--max-time", "10", "-X", "POST", &format!("{base}/navigate/site/{site_id}")])
                .output()
                .map_err(|e| format!("navigate_to_site curl: {e}"))?;
            let body = String::from_utf8_lossy(&out.stdout);
            if body.contains("\"navigated\"") {
                wait_for_idle_after_navigate(dev);
                Ok(format!("navigated to site {site_id}"))
            } else {
                Err(format!("navigate_to_site failed: {}", body.trim()))
            }
        }
        "navigate_to_user" => {
            let user_id = action.user_id
                .map(|id| id.to_string())
                .or_else(|| ctx.vars.get("user_id").and_then(|v| v.as_str().map(|s| s.to_string())).or_else(|| ctx.vars.get("user_id").and_then(|v| v.as_i64().map(|n| n.to_string()))))
                .ok_or("navigate_to_user: no user_id")?;
            let base = agent_base_url();
            let out = std::process::Command::new("curl")
                .args(["-s", "--max-time", "10", "-X", "POST", &format!("{base}/navigate/user/{user_id}")])
                .output()
                .map_err(|e| format!("navigate_to_user curl: {e}"))?;
            let body = String::from_utf8_lossy(&out.stdout);
            if body.contains("\"navigated\"") {
                wait_for_idle_after_navigate(dev);
                Ok(format!("navigated to user {user_id}"))
            } else {
                Err(format!("navigate_to_user failed: {}", body.trim()))
            }
        }
        "deep_link" => {
            Err(format!("{}: use platform: android: steps in TC YAML", action.action))
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
            let base = std::env::var("DDB_API_BASE_URL").unwrap_or_default();
            let full_url = if url.starts_with("http") { url.to_string() } else { format!("{base}{url}") };

            let mut curl_args = vec![
                "-s".to_string(),
                "-w".to_string(), "\n%{http_code}".to_string(),
                "-X".to_string(), method.to_uppercase(),
                "--connect-timeout".to_string(), "10".to_string(),
            ];

            if let Some(ref hdrs) = action.headers {
                for (k, v) in hdrs {
                    curl_args.push("-H".to_string());
                    curl_args.push(format!("{}: {}", k, ctx.interpolate(v)));
                }
            }

            if let Some(ref body) = action.body {
                let body_str = ctx.interpolate(&body.to_string());
                curl_args.push("-H".to_string());
                curl_args.push("Content-Type: application/json".to_string());
                curl_args.push("-d".to_string());
                curl_args.push(body_str);
            }

            curl_args.push(full_url.clone());

            let output = std::process::Command::new("curl")
                .args(&curl_args)
                .output()
                .map_err(|e| format!("api_call curl: {e}"))?;

            let raw = String::from_utf8_lossy(&output.stdout).to_string();
            let (body_str, status_str) = raw.rsplit_once('\n').unwrap_or((&raw, "0"));
            let status: u16 = status_str.trim().parse().unwrap_or(0);

            if status >= 400 || status == 0 {
                return Err(format!("api_call {method} {full_url}: HTTP {status} — {}", &body_str[..body_str.len().min(200)]));
            }

            if let Some(ref save_key) = action.save_as {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body_str) {
                    ctx.vars.insert(save_key.clone(), json_val);
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
            let target = assert.target.as_ref();
            let expected_text = assert.text.as_deref();
            let expected_hint = assert.hint.as_deref();

            // Poll uiautomator + semantic agent in parallel loop (catches AlertDialogs + async content)
            let fuzzy = target.and_then(|t| t.content_fuzzy.as_deref());
            let id = target.and_then(|t| t.id.as_deref());
            for poll in 0..10 {
                // Check uiautomator (fast, catches system dialogs)
                let ui_xml = fetch_ui_dump(dev);
                let ui_lower = ui_xml.to_lowercase();
                let found_ui = fuzzy.map(|f| ui_lower.contains(&f.to_lowercase())).unwrap_or(false)
                    || id.map(|i| ui_xml.contains(i)).unwrap_or(false)
                    || expected_text.map(|t| ui_lower.contains(&t.to_lowercase())).unwrap_or(false);
                if found_ui {
                    return Ok(format!("found in uiautomator (poll {})", poll));
                }
                // Check accessibility dump (catches AlertDialog content that uiautomator dump misses)
                if let Ok(a11y_dump) = adb::shell(dev, &["dumpsys", "activity", "top"]) {
                    let a11y_lower = a11y_dump.to_lowercase();
                    let found_a11y = fuzzy.map(|f| a11y_lower.contains(&f.to_lowercase())).unwrap_or(false);
                    if found_a11y {
                        return Ok(format!("found in activity dump (poll {})", poll));
                    }
                }
                // Check semantic agent (full dump, catches app content)
                if let Ok(elements) = get_semantic_elements(dev) {
                    let found_agent = elements.iter().any(|e| {
                        let e_lower = e.to_lowercase();
                        fuzzy.map(|f| e_lower.contains(&f.to_lowercase())).unwrap_or(false)
                            || id.map(|i| e.contains(i)).unwrap_or(false)
                    });
                    if found_agent {
                        let content = elements.iter()
                            .find(|e| fuzzy.map(|f| e.to_lowercase().contains(&f.to_lowercase())).unwrap_or(false))
                            .and_then(|e| e.lines().find(|l| l.trim().starts_with("content:")).map(|l| l.trim().to_string()))
                            .unwrap_or_default();
                        return Ok(format!("found: {content}"));
                    }
                }
                if poll < 9 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            return Err(format!("element not found after 10 polls: {:?}", fuzzy.or(id)));
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

    // Fallback: check uiautomator dump for dialog elements
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
    let (x1, y1, x2, y2) = compute_scroll_bounds(dev, dir);
    adb::shell(dev, &[
        "input", "swipe",
        &x1.to_string(), &y1.to_string(),
        &x2.to_string(), &y2.to_string(),
        "500",
    ])?;
    Ok(())
}

fn is_page_stable(dev: Option<&Device>) -> Option<bool> {
    let count1 = fetch_agent_yaml_full(dev).ok()
        .map(|y| y.matches("\n- ").count());
    std::thread::sleep(std::time::Duration::from_secs(2));
    let count2 = fetch_agent_yaml_full(dev).ok()
        .map(|y| y.matches("\n- ").count());
    match (count1, count2) {
        (Some(c1), Some(c2)) if c1 > 0 => Some(c1 == c2),
        _ => None,
    }
}

fn compute_scroll_bounds(dev: Option<&Device>, dir: &str) -> (i32, i32, i32, i32) {
    // Try to find scrollable container bounds from semantic dump
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
                if w > 100 && h > 200 {
                    let cx = x + w / 2;
                    let top = y + h / 4;
                    let bot = y + h * 3 / 4;
                    return match dir {
                        "down" => (cx, bot, cx, top),
                        "up" => (cx, top, cx, bot),
                        "left" => (bot, cx, top, cx),
                        "right" => (top, cx, bot, cx),
                        _ => (540, 1800, 540, 900),
                    };
                }
            }
        }
    }
    // Fallback: screen center
    match dir {
        "down" => (540, 1800, 540, 900),
        "up" => (540, 900, 540, 1800),
        "left" => (800, 1100, 200, 1100),
        "right" => (200, 1100, 800, 1100),
        _ => (540, 1800, 540, 900),
    }
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

fn wait_for_idle_after_navigate(dev: Option<&Device>) {
    std::thread::sleep(std::time::Duration::from_secs(2));
    wait_idle(dev, 10);
    std::thread::sleep(std::time::Duration::from_secs(1));
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
        .args(["-s", "--connect-timeout", "1", "--max-time", "5", &format!("{}/idle", agent_base_url())])
        .output()
        .map_err(|e| format!("curl error: {e}"))?;

    let body = String::from_utf8_lossy(&resp.stdout);
    Ok(body.contains("\"idle\":true") || body.contains("\"idle\": true"))
}

fn fetch_agent_yaml_full_with_retry(dev: Option<&Device>) -> Result<String, String> {
    for _ in 0..3 {
        if let Ok(yaml) = fetch_agent_yaml_full(dev) {
            return Ok(yaml);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    fetch_agent_yaml(dev)
}

fn fetch_agent_yaml_full(_dev: Option<&Device>) -> Result<String, String> {
    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "5", "--max-time", "15", &format!("{}/semantic?scroll=0", agent_base_url())])
        .output()
        .map_err(|e| format!("curl error: {e}"))?;
    if !resp.status.success() {
        return Err("agent not responding (full dump)".to_string());
    }
    let body = String::from_utf8_lossy(&resp.stdout).into_owned();
    if body.contains("elements:") { Ok(body) } else { Err("invalid agent response (full)".to_string()) }
}

fn fetch_agent_yaml(dev: Option<&Device>) -> Result<String, String> {
    let resp = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "2", "--max-time", "10", &format!("{}/semantic", agent_base_url())])
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
    let yaml = fetch_agent_yaml_full_with_retry(dev)?;
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

fn extract_yaml_int_after(chunk: &str, section: &str, key: &str) -> Option<i32> {
    let section_pos = chunk.find(section)?;
    let after = &chunk[section_pos..];
    for line in after.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || (!trimmed.starts_with(key) && !trimmed.contains(": ")) {
            break;
        }
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

fn fetch_debug_log() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "2", &format!("{}/debug-log", agent_base_url())])
        .output()
        .ok()?;
    let body = String::from_utf8_lossy(&output.stdout).to_string();
    if body.is_empty() { None } else { Some(body) }
}

fn capture_failure_screenshot(dev: Option<&Device>, test_id: &str, step: usize) -> Option<String> {
    let path = format!("/tmp/ddb-test-fail-{}-step{}.png", test_id, step);
    let _ = adb::shell(dev, &["screencap", "-p", "/sdcard/fail.png"]);
    let _ = adb::adb(dev, &["pull", "/sdcard/fail.png", &path]);
    Some(path)
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    #[test]
    fn test_platform_fork_parsing() {
        let yaml = r#"
id: TC-19
name: "test"
steps:
  - action: navigate_to_site
    site_id: 31255
    platform:
      android:
        - action: tap
          target: {content_fuzzy: "search"}
        - action: wait
          seconds: 2
      ios:
        - action: tap
          target: {content_fuzzy: "search ios"}
  - action: scroll_to
    target: {content_fuzzy: "questions"}
"#;
        let raw: TestSpecRaw = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(raw.steps.len(), 2, "should have 2 raw steps");
        
        let first = &raw.steps[0];
        assert!(first.platform.is_some(), "first step should have platform");
        let plat = first.platform.as_ref().unwrap();
        assert!(plat.android.is_some(), "should have android sub-steps");
        let android = plat.android.as_ref().unwrap();
        assert_eq!(android.len(), 2, "android should have 2 sub-steps");
        assert_eq!(android[0].action.as_deref(), Some("tap"));
        
        // Test expansion
        let expanded: Vec<StepRaw> = raw.steps.into_iter()
            .flat_map(|s| {
                let action_name = s.action.as_deref().unwrap_or("");
                if action_name == "navigate_to_site" || action_name == "navigate_to_user" {
                    return vec![s];
                }
                if let Some(ref plat) = s.platform {
                    if let Some(android_steps) = &plat.android {
                        return android_steps.clone();
                    }
                }
                vec![s]
            })
            .collect();
        assert_eq!(expanded.len(), 3, "expanded should have 3 steps (2 android + 1 scroll)");
        assert_eq!(expanded[0].action.as_deref(), Some("tap"));
        assert_eq!(expanded[2].action.as_deref(), Some("scroll_to"));
    }
}

fn switchboard_notify(msg: &str) {
    let handle = std::env::var("SWITCHBOARD_NAME").unwrap_or_default();
    let channel = std::env::var("SWITCHBOARD_CHANNEL").unwrap_or_default();
    if handle.is_empty() || channel.is_empty() { return; }
    let _ = std::process::Command::new("switchboard")
        .env("SWITCHBOARD_NAME", &handle)
        .env("SWITCHBOARD_CHANNEL", &channel)
        .args(["send", msg])
        .output();
}
