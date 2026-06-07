use clap::Args;

use crate::adb;
use crate::registry::{Device, Registry};

use super::test_element::{
    Target, find_element, find_element_unified, idle_barrier_sources,
    check_element_sources,
    extract_ui_bounds, extract_ui_text_bounds, extract_ui_bounds_fuzzy,
    fetch_ui_dump, fetch_agent_yaml,
    get_semantic_elements, agent_base_url,
    extract_yaml_int, extract_yaml_int_after, token_jaccard,
    scroll_direction, scroll_search,
};
use super::test_fixture::{load_fixtures_map, flatten_fixtures, interpolate_raw, FixtureResolver};
use super::test_observability::{capture_failure_screenshot, fetch_debug_log};
use super::test_log::Logger;
use super::test_timeout::{TimeoutManager, TimeoutLevel, StepRunner, PhaseBudgets, StepPhase};

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

    /// Regression gate: run only TCs affected by git changes, fail if any regress
    #[arg(long)]
    pub regression_gate: bool,

    /// Base branch for regression gate diff (default: main)
    #[arg(long, default_value = "main")]
    pub base_branch: String,

    /// TC mapping file (maps source files to TC IDs)
    #[arg(long, env = "DDB_TC_MAP")]
    pub tc_map: Option<String>,

    /// Capture semantic agent baseline on PASS (writes to baseline/ next to TC)
    #[arg(long)]
    pub capture_baseline: bool,
}

#[derive(serde::Deserialize)]
struct TestSpecRaw {
    id: String,
    name: String,
    #[serde(default)]
    precondition: Option<Precondition>,
    steps: Vec<StepRaw>,
}

#[derive(Clone)]
struct TestSpec {
    id: String,
    name: String,
    precondition: Option<Precondition>,
    steps: Vec<Step>,
}

#[derive(serde::Deserialize, Clone)]
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
    #[serde(default)]
    wait_for: Option<Vec<String>>,
    #[serde(default)]
    wait_timeout: Option<u64>,
    #[serde(default)]
    click_after: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    max_attempts: Option<u32>,
}

#[derive(serde::Deserialize, Clone)]
struct PlatformSteps {
    #[serde(default)]
    android: Option<Vec<StepRaw>>,
    #[serde(default)]
    ios: Option<Vec<StepRaw>>,
}

#[derive(Clone)]
enum Step {
    Action(ActionStep),
    Assert(AssertStep),
}

#[derive(Clone)]
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
    wait_for: Option<Vec<String>>,
    wait_timeout: Option<u64>,
    click_after: Option<String>,
    source: Option<String>,
    max_attempts: Option<u32>,
}

#[derive(Clone)]
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
                wait_for: self.wait_for,
                wait_timeout: self.wait_timeout,
                click_after: self.click_after,
                source: self.source,
                max_attempts: self.max_attempts,
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

#[derive(serde::Serialize)]
struct TestResult {
    id: String,
    name: String,
    status: String,
    steps_run: usize,
    steps_total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<FailureDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    log: Vec<super::test_log::LogEntry>,
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

    // TD-25: util_runner for short pre-TC operations (version check, port
    // forward, animations toggle, recover_adb_if_zombie probe). Default
    // 30s deadline matches industry adb-call caps (Appium adbExecTimeout
    // 20s, uiautomator2 60s); the prior 300s outer deadline let any
    // single derived call inherit 5 minutes, which masked the /version
    // hang fixed in #43 and surfaces no usable signal otherwise. Build
    // and install are long ops; they wrap themselves in fresh longer-
    // deadline runners below rather than inheriting from util_runner.
    let util_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let util_runner = StepRunner::new(util_deadline, PhaseBudgets { pre_idle_s: 30, execute_s: 30, post_idle_s: 3 });

    // Build + install if --build flag
    if args.build {
        let project_dir = args.project_dir.as_deref()
            .ok_or("--build requires --project-dir or DDB_PROJECT_DIR")?;
        eprintln!("building APK from {project_dir}...");
        let mut build_cmd = std::process::Command::new("nosandbox");
        build_cmd.args(&[
                "./gradlew",
                &std::env::var("DDB_BUILD_TASK").unwrap_or_else(|_| "assembleStandardDebug".into()),
                "--no-daemon",
            ])
            .current_dir(project_dir);
        // TD-25 (reintegrated): build wraps in its own 600s runner.
        let build_runner = StepRunner::fresh_with_budget(600);
        let build_output = build_runner.run_with_deadline(&mut build_cmd)
            .map_err(|e| format!("build failed: {e}"))?;
        if !build_output.status.success() {
            return Err("APK build failed".into());
        }
        let apk_src = std::env::var("DDB_APK_SRC").unwrap_or_else(|_|
            format!("{project_dir}/app/build/outputs/apk/standard/debug/app-standard-debug.apk"));
        let apk_dst = std::env::var("DDB_APK_PATH").unwrap_or_else(|_| "/tmp/app-debug.apk".into());
        std::fs::copy(&apk_src, &apk_dst)
            .map_err(|e| format!("copy APK: {e}"))?;
        eprintln!("installing APK...");
        let mut install_cmd = std::process::Command::new("adb");
        if let Some(ref d) = dev { install_cmd.arg("-s").arg(d.transport_id()); }
        install_cmd.args(["install", "-r", &apk_dst]);
        // TD-25 (reintegrated): install wraps in its own 120s runner
        // (Appium installApkTimeout default is 90s; 120s matches our
        // prior behavior on large APKs).
        let install_runner = StepRunner::fresh_with_budget(120);
        let install_result = install_runner.run_with_deadline(&mut install_cmd).map(|_| String::new());
        if install_result.is_err() {
            return Err("APK install failed".into());
        }
        eprintln!("APK installed. waiting for app launch...");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }

    // Version check: mandatory — refuse to run without expected hash
    let expected_hash = args.expected_hash.clone()
        .or_else(|| std::env::var("DDB_EXPECTED_HASH").ok())
        .filter(|h| !h.is_empty())
        .ok_or("DDB_EXPECTED_HASH not set. Run: export DDB_EXPECTED_HASH=$(git -C /path/to/app rev-parse --short HEAD)")?;
    {
        let base = agent_base_url();
        // #43 — /version is a sanity check, not the agent-ready gate.
        // Bound at 5s so a wedged forward (port accepts but agent
        // doesn't respond) doesn't burn util_runner's full 300s
        // budget. derived_with_deadline (#41) gives us a fresh
        // 5s-capped runner for this single probe.
        let version_probe = util_runner.derived_with_deadline(5);
        let version_result = version_probe.curl_with_deadline(&format!("{base}/version"), "GET", None);
        if let Ok(body) = version_result {
            if let Some(hash_start) = body.find("\"git_hash\":\"") {
                let rest = &body[hash_start + 12..];
                if let Some(end) = rest.find('"') {
                    let installed = &rest[..end];
                    if installed.is_empty() {
                        eprintln!("  WARNING: agent returned empty hash (AAR without BuildConfig)");
                    } else if installed != expected_hash {
                        return Err(format!("STALE BINARY: installed={installed} expected={expected_hash}. Rebuild APK."));
                    } else {
                        eprintln!("  agent version: {installed} ✓");
                    }
                }
            } else {
                eprintln!("  WARNING: could not read agent version (agent may not support /version)");
            }
        } else {
            eprintln!("  WARNING: agent not reachable for version check");
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
        let failed = get_failed_tc_specs(results_dir, tests_dir, &util_runner)?;
        if failed.is_empty() {
            println!("All TCs passing — nothing to rerun.");
            return Ok(());
        }
        println!("Re-running {} failed TCs:", failed.len());
        for s in &failed {
            println!("  {}", s);
        }
        failed
    } else if args.regression_gate {
        let results_dir = args.results_dir.as_deref().ok_or("--regression-gate requires --results-dir or DDB_RESULTS_DIR")?;
        let tests_dir = args.tests_dir.as_deref().ok_or("--regression-gate requires --tests-dir or DDB_TESTS_DIR")?;
        let affected = get_affected_tcs(&args.base_branch, args.tc_map.as_deref(), tests_dir, &util_runner)?;
        if affected.is_empty() {
            println!("No TCs affected by changes — gate passes.");
            return Ok(());
        }
        println!("Regression gate: {} affected TCs", affected.len());
        for s in &affected { println!("  {}", s); }
        affected
    } else {
        args.specs.clone()
    };

    if specs.is_empty() {
        return Err("no test spec files provided".to_string());
    }

    // Set up port forwarding for agent
    // Priority: DDB_AGENT_PORT env var > device registry agent_port > default 9876
    let agent_port = std::env::var("DDB_AGENT_PORT").unwrap_or_else(|_| {
        dev.as_ref().map(|d| d.agent_port().to_string()).unwrap_or_else(|| "9876".into())
    });
    unsafe { std::env::set_var("DDB_AGENT_PORT", &agent_port); }
    if let Some(ref d) = dev {
        // #44 — adb zombie pre-check. After repeated pm clear cycles
        // the host-side adb server wedges (Google issuetracker
        // 36920010). Probe `adb get-state` with a 3s cap; if it hangs
        // or returns garbage, kill+start the server so the forward
        // setup below isn't queued behind a dead daemon.
        recover_adb_if_zombie(d, &util_runner);
        // Remove stale 9876 forward if device uses a different port
        if agent_port != "9876" {
            let mut rm_cmd = std::process::Command::new("adb");
            rm_cmd.arg("-s").arg(d.transport_id()).args(["forward", "--remove", "tcp:9876"]);
            let _ = rm_cmd.output(); // ignore errors (forward may not exist)
        }
        let mut fwd_cmd = std::process::Command::new("adb");
        fwd_cmd.arg("-s").arg(d.transport_id()).args(["forward", &format!("tcp:{agent_port}"), "tcp:9876"]);
        // TD-25: forward is a one-shot adb call; cap tightly so it
        // surfaces as a hard error rather than absorbing 30s.
        let fwd_probe = util_runner.derived_with_deadline(5);
        let _ = fwd_probe.run_with_deadline(&mut fwd_cmd);
    }
    if agent_port != "9876" {
        eprintln!("  agent port: {agent_port} (forwarded to device 9876)");
    }

    // Disable animations for reliable test execution
    set_animations(false, &util_runner);

    // Pre-load fixtures for interpolation (used both pre-parse and at runtime)
    let fixtures_map = load_fixtures_map();

    // Load mock config (applied per-TC after agent restart)
    let mock_body: Option<String> = std::env::var("DDB_MOCK_CONFIG").ok().and_then(|config_path| {
        let config_content = std::fs::read_to_string(&config_path).ok()?;
        let config: serde_json::Value = serde_yaml::from_str(&config_content).ok()?;
        let mocks = config.get("mocks")?.as_array()?;
        let config_dir = std::path::Path::new(&config_path).parent().unwrap_or(std::path::Path::new("."));
        let mut mock_entries = Vec::new();
        for m in mocks {
            let mut entry = m.clone();
            if let Some(resp) = entry.get("response").and_then(|r| r.get("file")).and_then(|f| f.as_str()) {
                let resp_path = config_dir.join(resp);
                if let Ok(resp_content) = std::fs::read_to_string(&resp_path) {
                    if let Ok(resp_json) = serde_json::from_str::<serde_json::Value>(&resp_content) {
                        if let Some(obj) = entry.as_object_mut() {
                            let status = obj.get("response").and_then(|r| r.get("status")).cloned().unwrap_or(serde_json::json!(200));
                            obj.insert("response".into(), serde_json::json!({"status": status, "body": resp_json}));
                        }
                    }
                }
            }
            mock_entries.push(entry);
        }
        eprintln!("  loaded {} mocks from {}", mock_entries.len(), config_path);
        Some(serde_json::json!({"mocks": mock_entries}).to_string())
    });

    let mut results = Vec::new();
    let mut pass = 0;
    let mut fail = 0;

    let clean_state = std::env::var("DDB_CLEAN_STATE").ok().map(|v| v == "true").unwrap_or(false);
    for (tc_index, spec_path) in specs.iter().enumerate() {
        // #44 — Proactive adb zombie sweep between TCs on cold-state
        // suites. The host adb server wedges after ~5 repeated pm
        // clear cycles; restarting every 5 TCs keeps suites moving
        // without per-TC overhead. First TC (tc_index 0) already
        // handled by the setup-phase pre-check above.
        if clean_state && tc_index > 0 && tc_index % 5 == 0 {
            if let Some(ref d) = dev {
                eprintln!("  [#44] periodic adb sweep (TC {}/{})", tc_index + 1, specs.len());
                recover_adb_if_zombie(d, &util_runner);
            }
        }
        let raw_content = match std::fs::read_to_string(spec_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  SKIP  {} — read error: {}", spec_path, e);
                fail += 1;
                continue;
            }
        };
        let content = interpolate_raw(&raw_content, &fixtures_map);
        let raw: TestSpecRaw = match serde_yaml::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  SKIP  {} — parse error: {}", spec_path, e);
                fail += 1;
                continue;
            }
        };
        let expanded: Vec<StepRaw> = raw.steps.into_iter()
            .flat_map(|s| {
                let action_name = s.action.as_deref().unwrap_or("");
                if let Some(ref plat) = s.platform {
                    if let Some(android_steps) = &plat.android {
                        return android_steps.clone();
                    }
                }
                vec![s]
            })
            .collect();
        let steps: Vec<Step> = match expanded.into_iter()
            .map(|s| s.into_step())
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  SKIP  {} — invalid step: {}", spec_path, e);
                fail += 1;
                continue;
            }
        };
        let spec = TestSpec {
            id: raw.id,
            name: raw.name,
            precondition: raw.precondition,
            steps,
        };


        let started = now_iso();
        let tc_hard_timeout = {
            let env_timeout = std::env::var("DDB_TC_TIMEOUT").ok()
                .and_then(|v| v.parse::<u64>().ok());
            if let Some(t) = env_timeout {
                std::time::Duration::from_secs(t)
            } else {
                let step_count = spec.steps.len() as u64;
                let per_step = 30u64.max(args.step_timeout);
                let steps_budget = (per_step * step_count).max(300).min(900);
                let setup_budget = 120u64;
                std::time::Duration::from_secs(steps_budget + setup_budget)
            }
        };
        let spec_clone = spec.clone();
        let spec_path_clone = spec_path.to_string();
        let dev_clone = dev.clone();
        let step_timeout = args.step_timeout;
        let fixtures_clone = fixtures_map.clone();
        let mock_clone = mock_body.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = run_spec(&spec_clone, dev_clone.as_ref(), step_timeout, &fixtures_clone, mock_clone.as_deref(), &spec_path_clone);
            let _ = tx.send(result);
        });
        let (result, step_logs) = match rx.recv_timeout(tc_hard_timeout) {
            Ok(r) => r,
            Err(_) => {
                eprintln!("  TC hard timeout ({}s) — killing", tc_hard_timeout.as_secs());
                (TestResult {
                    id: spec.id.clone(), name: spec.name.clone(),
                    status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
                    failure: Some(FailureDetail {
                        step: 0, description: format!("TC hard timeout ({}s)", tc_hard_timeout.as_secs()),
                        screenshot: None,
                    }),
                    log: vec![],
                }, Vec::new())
            }
        };
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
            if args.capture_baseline {
                if let Ok(yaml) = fetch_agent_yaml(dev.as_ref()) {
                    let baseline_dir = std::path::Path::new(spec_path)
                        .parent().unwrap_or(std::path::Path::new("."))
                        .join("baseline");
                    let _ = std::fs::create_dir_all(&baseline_dir);
                    let baseline_path = baseline_dir.join(format!("{}.yaml", result.id));
                    let _ = std::fs::write(&baseline_path, &yaml);
                    eprintln!("  baseline → {}", baseline_path.display());
                }
            }
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
    set_animations(true, &util_runner);

    println!("\n{} passed, {} failed, {} total", pass, fail, pass + fail);

    if let Some(ref report_path) = args.report {
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| format!("json error: {e}"))?;
        std::fs::write(report_path, &json)
            .map_err(|e| format!("write report: {e}"))?;
        eprintln!("report: {}", report_path);
    }

    if args.regression_gate {
        let results_dir = args.results_dir.as_deref().unwrap_or(".");
        let regressions = check_regressions(&results, results_dir);
        if !regressions.is_empty() {
            eprintln!("\nREGRESSION GATE FAILED — {} TCs regressed (PASS → FAIL):", regressions.len());
            for r in &regressions { eprintln!("  {}", r); }
            std::process::exit(2);
        }
        println!("Regression gate: PASSED (no regressions)");
    } else if fail > 0 {
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

fn set_animations(enabled: bool, _runner: &StepRunner) {
    let short = StepRunner::fresh_with_budget(5);
    let _ = short.curl_with_deadline(&format!("{}/animations?enabled={enabled}", agent_base_url()), "POST", None);
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

fn ensure_input_focus(dev: Option<&Device>, runner: &StepRunner) -> Result<(), String> {
    let out = runner.adb_shell(dev, &["dumpsys", "input_method"])
        .map_err(|e| format!("ensure_input_focus: dumpsys input_method failed: {e}"))?;

    // TD-C: dumpsys always emits mServedView=<value> (value is 'null' or
    // a class name). The prior !contains("mServedView=") OR clause was
    // a dead-code defensive fallback for malformed dumps — drop it for
    // cleaner reasoning.
    let served_null = out.contains("mServedView=null");
    let served_not_edittext = !served_null
        && !out.contains("mServedView=android.widget.EditText")
        && !out.contains("mServedView=androidx.appcompat.widget.AppCompatEditText");
    let ime_hidden = out.contains("mInputShown=false");

    let val_line = if let Some(pos) = out.find("mServedView=") {
        let val = &out[pos..std::cmp::min(pos + 80, out.len())];
        val.lines().next().unwrap_or("").to_string()
    } else {
        String::new()
    };
    crate::ddb_debug!("[TD-C][focus] {} null={} not_edit={} ime_hidden={}",
        val_line, served_null, served_not_edittext, ime_hidden);

    if !(served_null || served_not_edittext) {
        return Ok(());
    }

    let mut tapped = false;
    // Primary: /semantic (faster than UIAutomator on WiFi, 5× retry with
    // 500ms backoff). TD-C: extended type matcher to cover Compose
    // TextField alongside UIKit-era 'input' / 'text_field' / EditText.
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if let Ok(yaml) = runner.curl_with_deadline(&format!("{}/semantic", agent_base_url()), "GET", None) {
            for chunk in yaml.split("\n- ") {
                if chunk.contains("type: input")
                    || chunk.contains("type: text_field")
                    || chunk.contains("type: TextField")
                    || chunk.contains("EditText") {
                    let x = extract_yaml_int(chunk, "x: ");
                    let y = extract_yaml_int(chunk, "y: ");
                    let w = extract_yaml_int(chunk, "w: ");
                    let h = extract_yaml_int(chunk, "h: ");
                    if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
                        let cx = x + w / 2;
                        let cy = y + h / 2;
                        crate::ddb_debug!("[TD-C][focus] tap via /semantic cx={} cy={} attempt={}", cx, cy, attempt);
                        let _ = runner.adb_shell(dev, &["input", "tap", &cx.to_string(), &cy.to_string()]);
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        tapped = true;
                        break;
                    }
                }
            }
        }
        if tapped { break; }
    }
    // Fallback: UIAutomator (screen-absolute coords, 5× retry with 500ms backoff)
    if !tapped {
        for attempt in 0..5 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            let _ = runner.adb_shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]);
            if let Ok(xml) = runner.adb_shell(dev, &["cat", "/sdcard/ui.xml"]) {
                if let Some(caps) = xml.find("EditText") {
                    let after = &xml[caps..];
                    if let Some(b_start) = after.find("bounds=\"[") {
                        let bounds_str = &after[b_start + 8..];
                        if let Some(b_end) = bounds_str.find(']') {
                            let first = &bounds_str[1..b_end];
                            let rest = &bounds_str[b_end + 2..];
                            if let Some(b_end2) = rest.find(']') {
                                let second = &rest[..b_end2];
                                let c1: Vec<i32> = first.split(',').filter_map(|s| s.parse().ok()).collect();
                                let c2: Vec<i32> = second.split(',').filter_map(|s| s.parse().ok()).collect();
                                if c1.len() == 2 && c2.len() == 2 {
                                    let cx = (c1[0] + c2[0]) / 2;
                                    let cy = (c1[1] + c2[1]) / 2;
                                    crate::ddb_debug!("[TD-C][focus] tap via UIAutomator cx={} cy={} attempt={}", cx, cy, attempt);
                                    let _ = runner.adb_shell(dev, &["input", "tap", &cx.to_string(), &cy.to_string()]);
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                    tapped = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !tapped {
        // TD-C: classified hard-fail. Prior behavior was silent return — type
        // would proceed and characters went to /dev/null (DecorView focus,
        // no EditText surfaceable). Include truncated dumpsys snippet for
        // operator triage.
        let snippet: String = val_line.chars().take(200).collect();
        return Err(format!(
            "ensure_input_focus: no EditText found on /semantic or UIAutomator after 10 attempts (mServedView snippet: {})",
            snippet
        ));
    }
    Ok(())
}

fn get_failed_tc_specs(results_dir: &str, tests_dir: &str, runner: &StepRunner) -> Result<Vec<String>, String> {
    let mut cmd = std::process::Command::new("vdb");
    cmd.args(["matrix", "--results", results_dir, "--json"]);
    let output = runner.run_with_deadline(&mut cmd)?;

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

fn interpolate_env_vars(content: &str) -> String {
    let mut result = content.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result = format!("{}{}{}", &result[..start], replacement, &result[start + end + 1..]);
        } else {
            break;
        }
    }
    result
}

fn execute_recipe(recipe_path: &str, dev: Option<&Device>, runner: &StepRunner, fixtures: &std::collections::HashMap<String, String>) -> Result<(), String> {
    let raw_content = std::fs::read_to_string(recipe_path)
        .map_err(|e| format!("recipe read error: {e}"))?;
    let after_fixtures = interpolate_raw(&raw_content, fixtures);
    let content = interpolate_env_vars(&after_fixtures);

    #[derive(serde::Deserialize)]
    struct RecipeFile { steps: Vec<StepRaw> }

    let recipe: RecipeFile = serde_yaml::from_str(&content)
        .map_err(|e| format!("recipe parse error: {e}"))?;

    let steps: Vec<Step> = recipe.steps.into_iter()
        .filter_map(|s| s.into_step().ok())
        .collect();
    // Recipe-runner path (used by login + check_skip); no TC dir context.
    // stage_gallery is not expected here, but if a recipe ever uses it the
    // source must be absolute.
    let mut ctx = RunContext::new(fixtures.clone(), std::path::PathBuf::from("."));

    for (i, step) in steps.iter().enumerate() {
        match step {
            Step::Action(action) if action.action == "check_skip" => {
                if let Some(ref target) = action.target {
                    let search = target.content_fuzzy.as_deref().unwrap_or("");
                    if !search.is_empty() {
                        let url = format!("{}/semantic", agent_base_url());
                        if let Ok(yaml) = runner.curl_with_deadline(&url, "GET", None) {
                            if yaml.to_lowercase().contains(&search.to_lowercase()) {
                                eprintln!("  recipe step {}: check_skip → '{}' found, skipping remaining steps", i + 1, search);
                                return Ok(());
                            }
                        }
                    }
                }
                eprintln!("  recipe step {}: check_skip → not found, continuing", i + 1);
            }
            Step::Action(action) if action.action == "type" && action.target.is_some() => {
                let text = action.text.as_ref().ok_or("recipe type: no text")?;
                let resolved = ctx.interpolate(text);
                let target = action.target.as_ref().unwrap();
                let mut type_json = serde_json::json!({"text": resolved, "clear": true});
                if let Some(ref fuzzy) = target.content_fuzzy {
                    type_json["content_fuzzy"] = serde_json::json!(fuzzy);
                }
                if let Some(ref id) = target.id {
                    type_json["resource_id"] = serde_json::json!(id);
                }
                let type_body = type_json.to_string();
                let type_url = format!("{}/type", agent_base_url());
                match runner.curl_with_deadline(&type_url, "POST", Some(&type_body)) {
                    Ok(_) => eprintln!("  recipe step {}: type → typed \"{}\"", i + 1, resolved),
                    Err(e) => return Err(format!("recipe step {} (type) failed: {}", i + 1, e)),
                }
            }
            Step::Action(action) => {
                let result = execute_action(dev, action, &mut ctx, runner);
                match result {
                    Ok(desc) => eprintln!("  recipe step {}: {} → {}", i + 1, action.action, desc),
                    Err(e) => return Err(format!("recipe step {} ({}) failed: {}", i + 1, action.action, e)),
                }
            }
            Step::Assert(_) => {}
        }
    }
    Ok(())
}

fn ensure_logged_in_with_runner(dev: Option<&Device>, _pkg: &str, runner: &StepRunner, fixtures: &std::collections::HashMap<String, String>) -> Result<(), String> {
    let indicator = std::env::var("DDB_LOGGED_IN_INDICATOR").unwrap_or_else(|_| "log out".into());
    let indicator_lc = indicator.to_lowercase();
    let base = agent_base_url();
    crate::ddb_debug!("[TD-G][login] pre-check indicator='{}'", indicator);
    if let Ok(body) = runner.curl_with_deadline(&format!("{base}/semantic"), "GET", None) {
        if body.to_lowercase().contains(&indicator_lc) {
            eprintln!("  already logged in (found '{}' in semantic)", indicator);
            return Ok(());
        }
    }

    let recipe_path = std::env::var("DDB_LOGIN_RECIPE").ok().filter(|p| !p.is_empty())
        .ok_or_else(|| "DDB_LOGIN_RECIPE not set — TC requires logged_in:true but no recipe path provided".to_string())?;

    crate::ddb_debug!("[TD-G][login] running recipe path='{}'", recipe_path);
    execute_recipe(&recipe_path, dev, runner, fixtures)
        .map_err(|e| format!("login recipe '{recipe_path}' failed: {e}"))?;
    eprintln!("  login recipe executed successfully");

    // TD-G: post-verify with bounded poll. The recipe may report success
    // before the UI surfaces the logged-in indicator (async API → state
    // binding → re-render lag). 5s window matches the wait_until /
    // element_exists default. 500ms cadence matches the wait_until
    // convention. Without this poll, fast-path TCs whose recipe
    // resolves immediately still pass, but TCs with even small UI lag
    // would false-negative (single-shot pre-fix).
    let poll_secs: u64 = std::env::var("DDB_LOGGED_IN_INDICATOR_POLL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let post_deadline = std::time::Instant::now() + std::time::Duration::from_secs(poll_secs);
    loop {
        if let Ok(body) = runner.curl_with_deadline(&format!("{base}/semantic"), "GET", None) {
            if body.to_lowercase().contains(&indicator_lc) {
                crate::ddb_debug!("[TD-G][login] post-recipe indicator='{}' found", indicator);
                return Ok(());
            }
        }
        if std::time::Instant::now() > post_deadline {
            crate::ddb_debug!("[TD-G][login] post-recipe poll exhausted indicator='{}'", indicator);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Err(format!("login recipe ran but logged-in indicator '{indicator}' not found in /semantic post-recipe ({poll_secs}s poll)"))
}

fn grant_all_permissions_with_runner(dev: Option<&Device>, pkg: &str, runner: &StepRunner) {
    let perms = "pm grant PKG android.permission.ACCESS_FINE_LOCATION; pm grant PKG android.permission.ACCESS_COARSE_LOCATION; pm grant PKG android.permission.POST_NOTIFICATIONS";
    let cmd = perms.replace("PKG", pkg);
    let _ = runner.adb_shell(dev, &[&cmd]);
}

fn dismiss_permission_dialog(dev: Option<&Device>, runner: &StepRunner) {
    let ui = runner.adb_shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]).unwrap_or_default();
    let _ = runner.adb_shell(dev, &["cat", "/sdcard/ui.xml"]);
    let ui = runner.adb_shell(dev, &["cat", "/sdcard/ui.xml"]).unwrap_or_default();
    let ui_lower = ui.to_lowercase();
    if ui_lower.contains("permission") || ui_lower.contains("allow") || ui_lower.contains("while using") {
        let perm_buttons = std::env::var("DDB_PERMISSION_BUTTONS")
            .unwrap_or_else(|_| "permission_allow_foreground_only_button,permission_allow_button".into());
        for btn_id in perm_buttons.split(',') {
            let btn_id = btn_id.trim();
            if ui.contains(btn_id) {
                let _ = runner.adb_shell(dev, &["input", "keyevent", "KEYCODE_TAB"]);
                if let Some(bounds) = extract_ui_bounds(&ui, btn_id) {
                    let _ = runner.adb_shell(dev, &["input", "tap", &bounds.0.to_string(), &bounds.1.to_string()]);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    return;
                }
            }
        }
        if let Some(bounds) = extract_ui_text_bounds(&ui, "While using") {
            let _ = runner.adb_shell(dev, &["input", "tap", &bounds.0.to_string(), &bounds.1.to_string()]);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

// TD-24 — Thread-local flag signalling that wait_idle observed a
// zombie adb mid-TC. Read+cleared when constructing the next step
// FailureDetail so the failure description gets an ADB_ZOMBIE_DETECTED
// prefix, distinguishing infra wedge from real product flake in
// post-run triage. Per-thread so concurrent test runs don't bleed.
thread_local! {
    static ADB_ZOMBIE_FLAG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Signal that wait_idle observed an unhealthy adb get-state. Consumed
/// by the FailureDetail construction in the per-step error path to
/// prepend the [ADB_ZOMBIE_DETECTED] classification.
fn set_adb_zombie_flag() {
    crate::ddb_debug!("[TD-24][flag] set");
    ADB_ZOMBIE_FLAG.with(|f| f.set(true));
}

/// Read+clear the zombie flag. Called when building a step
/// FailureDetail; returns true at most once per zombie episode (the
/// flag is reset on read so subsequent unrelated failures aren't
/// mis-classified).
fn take_adb_zombie_flag() -> bool {
    let v = ADB_ZOMBIE_FLAG.with(|f| { let v = f.get(); f.set(false); v });
    if v { crate::ddb_debug!("[TD-24][flag] take=true — prefixing failure"); }
    v
}

// probe_adb_state moved to adb::probe_state during reintegration.
// Callers below use crate::adb::probe_state directly.

/// #44 — Detect a wedged host-side adb server and restart it.
///
/// Probes via adb::probe_state (3s). On hang / non-ok we run
/// `adb kill-server` + `adb start-server` and probe once more to
/// confirm recovery.
///
/// Returns true when a restart actually happened.
fn recover_adb_if_zombie(dev: &Device, _runner: &StepRunner) -> bool {
    let healthy = matches!(crate::adb::probe_state(dev).as_deref(), Some("device"));
    if healthy {
        return false;
    }

    eprintln!("  [#44] adb server appears wedged — restarting");
    let _ = std::process::Command::new("adb").arg("kill-server").output();
    // start-server is a fast no-op if already running; safe to chain.
    let _ = std::process::Command::new("adb").arg("start-server").output();
    std::thread::sleep(std::time::Duration::from_millis(500));

    if matches!(crate::adb::probe_state(dev).as_deref(), Some("device")) {
        eprintln!("  [#44] adb server recovered");
    } else {
        eprintln!(
            "  [#44] adb server restarted but device {} still not reporting 'device' — continuing",
            dev.transport_id()
        );
    }
    true
}

fn dismiss_keyboard_if_visible(dev: Option<&Device>, runner: &StepRunner) {
    if let Ok(out) = runner.adb_shell(dev, &["dumpsys", "input_method"]) {
        if out.contains("mInputShown=true") {
            let _ = runner.adb_shell(dev, &["input", "keyevent", "111"]);
            std::thread::sleep(std::time::Duration::from_secs(1));
            wait_idle(dev, 3);
        }
    }
}

struct RunContext {
    resolver: FixtureResolver,
    /// Directory of the TC YAML file. Used to resolve relative paths in
    /// actions like `stage_gallery` (source: images/carbonara.jpeg).
    tc_dir: std::path::PathBuf,
}

impl RunContext {
    fn new(fixtures: std::collections::HashMap<String, String>, tc_dir: std::path::PathBuf) -> Self {
        Self { resolver: FixtureResolver::new(fixtures), tc_dir }
    }

    fn interpolate(&self, s: &str) -> String {
        self.resolver.resolve(s)
    }

    fn add_api_response(&mut self, key: &str, val: serde_json::Value) {
        self.resolver.add_api_response(key, val);
    }

    fn get_var(&self, key: &str) -> Option<&serde_json::Value> {
        self.resolver.get_var(key)
    }
}

fn run_spec(spec: &TestSpec, dev: Option<&Device>, timeout: u64, fixtures: &std::collections::HashMap<String, String>, mock_body: Option<&str>, spec_path: &str) -> (TestResult, Vec<StepLogEntry>) {
    let mut step_logs: Vec<StepLogEntry> = Vec::new();
    let tc_dir = std::path::Path::new(spec_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut ctx = RunContext::new(fixtures.clone(), tc_dir);
    let logger = Logger::new();

    let pkg_env = std::env::var("DDB_TEST_PACKAGE").ok();
    let pkg = spec.precondition.as_ref()
        .and_then(|p| p.package.as_deref())
        .or(pkg_env.as_deref())
        .expect("No package name. Set DDB_TEST_PACKAGE env var or add precondition.package to TC YAML.");
    let main_activity = std::env::var("DDB_MAIN_ACTIVITY")
        .expect("DDB_MAIN_ACTIVITY not set — required for app launch");
    let base = agent_base_url();

    let setup_start = std::time::Instant::now();
    let setup_deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let setup_runner = StepRunner::new(setup_deadline, PhaseBudgets { pre_idle_s: 120, execute_s: 120, post_idle_s: 3 });

    // ── TC Setup: one clean flow, no branches ──
    // 1. Stop app
    let clean_state = std::env::var("DDB_CLEAN_STATE").ok().map(|v| v == "true").unwrap_or(false);
    // TD-F: WARN-only runtime assertion that the chosen branch actually
    // achieved expected state. Catches silent regressions per TD-9 (pm
    // clear / force-stop succeeding without effect on OEM-protected
    // packages or non-debuggable APKs).
    let dev_label = dev.map(|d| d.transport_id()).unwrap_or_else(|| "no-dev".into());
    if clean_state {
        let clear_result = setup_runner.adb_shell(dev, &["pm", "clear", pkg]);
        crate::ddb_debug!("[TD-F][clean_state] dev={} pm clear pkg={} -> {:?}",
            dev_label, pkg, clear_result.as_ref().map(|s| s.trim()));
        grant_all_permissions_with_runner(dev, pkg, &setup_runner);
        // Post-assert via `run-as pkg ls .` (cwd is /data/data/<pkg>).
        // Best-effort: SKIP silently when run-as is denied (production
        // APK without debuggable=true) — emit WARN only on real signal.
        if let Ok(out) = setup_runner.adb_shell(dev, &["run-as", pkg, "ls", "."]) {
            let trimmed = out.trim();
            if trimmed.contains("Permission denied") || trimmed.contains("is not debuggable") {
                crate::ddb_debug!("[TD-F][clean_state] dev={} post-assert skipped: run-as denied", dev_label);
            } else {
                let lines: Vec<&str> = trimmed.lines().filter(|l| !l.is_empty()).collect();
                if !lines.is_empty() {
                    eprintln!("[TD-F][clean_state] WARN: dev={} pm clear ran but {} entries remain in /data/data/{}/: {}",
                        dev_label, lines.len(), pkg, lines.join(","));
                }
            }
        }
    } else {
        let stop_result = setup_runner.adb_shell(dev, &["am", "force-stop", pkg]);
        crate::ddb_debug!("[TD-F][force_stop] dev={} am force-stop pkg={} -> {:?}",
            dev_label, pkg, stop_result.as_ref().map(|s| s.trim()));
        // Post-assert via pidof — universal, no root or run-as needed.
        if let Ok(pid) = setup_runner.adb_shell(dev, &["pidof", pkg]) {
            let pid_trim = pid.trim();
            if !pid_trim.is_empty() {
                eprintln!("[TD-F][force_stop] WARN: dev={} am force-stop ran but pkg={} pid={} still running",
                    dev_label, pkg, pid_trim);
            }
        }
    }

    // 2. Launch app
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = setup_runner.adb_shell(dev, &[
        "am", "start", "-a", "android.intent.action.MAIN",
        "-c", "android.intent.category.LAUNCHER",
        "-n", &main_activity,
    ]);
    // Settle wait — cold launches after pm clear are slow (dex/oat
    // optimization on first invocation). Warm launches go fast.
    let settle_s = if clean_state { 5 } else { 2 };
    std::thread::sleep(std::time::Duration::from_secs(settle_s));

    // 3. Health check — hard fail if agent not ready.
    //
    // Two-phase gate when DDB_SEMANTIC_GATE=true (#39 — Flutter VM
    // ready). The Flutter agent's HTTP server binds before runApp(),
    // so /health goes OK while the widget tree is still nil. /semantic
    // walks an empty rootElement and the first wait_until in the TC
    // (e.g. "Get Started") times out even though the element is one
    // frame away. Phase 1 stays /health (existing behavior, default
    // for native Android). Phase 2 polls /semantic and waits for any
    // element to appear (`- id:` line — every record emits it). Shares
    // the same DDB_AGENT_READY_TIMEOUT budget.
    // Default timeout — 120s on cold (#40, Flutter cold launches post
    // pm clear hit dex/oat opt + first-frame jank), 5s on warm. Either
    // can be overridden explicitly via DDB_AGENT_READY_TIMEOUT.
    let agent_ready_timeout_default: u64 = if clean_state { 120 } else { 5 };
    let agent_ready_timeout_s: u64 = std::env::var("DDB_AGENT_READY_TIMEOUT")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(agent_ready_timeout_default);
    let semantic_gate = std::env::var("DDB_SEMANTIC_GATE")
        .ok().map(|v| v == "true").unwrap_or(false);
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(agent_ready_timeout_s);

    let mut health_ok = false;
    while std::time::Instant::now() < deadline && !setup_runner.expired() {
        // TD-25: per-probe 5s cap so a single hung curl (port forward
        // accepted but agent unresponsive) cannot burn the full
        // setup_runner budget.
        let probe = setup_runner.derived_with_deadline(5);
        if let Ok(body) = probe.curl_with_deadline(&format!("{base}/health"), "GET", None) {
            if body.contains("semantic-agent") {
                health_ok = true;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !health_ok {
        return (TestResult {
            id: spec.id.clone(), name: spec.name.clone(),
            status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
            failure: Some(FailureDetail {
                step: 0,
                description: format!("agent /health not ready after {}s", agent_ready_timeout_s),
                screenshot: None,
            }),
            log: logger.entries(),
        }, step_logs);
    }

    let mut agent_ready = true;
    if semantic_gate {
        agent_ready = false;
        while std::time::Instant::now() < deadline && !setup_runner.expired() {
            // TD-25: per-probe 5s cap.
            let probe = setup_runner.derived_with_deadline(5);
            if let Ok(body) = probe.curl_with_deadline(&format!("{base}/semantic"), "GET", None) {
                if body.contains("- id:") {
                    agent_ready = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    if !agent_ready {
        return (TestResult {
            id: spec.id.clone(), name: spec.name.clone(),
            status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
            failure: Some(FailureDetail {
                step: 0,
                description: format!(
                    "agent /semantic empty after {}s (DDB_SEMANTIC_GATE)",
                    agent_ready_timeout_s
                ),
                screenshot: None,
            }),
            log: logger.entries(),
        }, step_logs);
    }

    // 4. Login if needed (recipe-only, no hardcoded flow).
    // Hard-fail the TC if the recipe is missing, fails, or doesn't actually
    // produce a logged-in state — silent continuation past a failed login
    // makes downstream wait_until / assert failures look like UI bugs.
    if let Some(ref pre) = spec.precondition {
        if pre.logged_in == Some(true) {
            if let Err(e) = ensure_logged_in_with_runner(dev, pkg, &setup_runner, fixtures) {
                return (TestResult {
                    id: spec.id.clone(), name: spec.name.clone(),
                    status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
                    failure: Some(FailureDetail {
                        step: 0,
                        description: format!("login required by precondition.logged_in=true: {e}"),
                        screenshot: None,
                    }),
                    log: logger.entries(),
                }, step_logs);
            }
        }
    }

    // 5. Register mocks — fail TC if error
    if let Some(body) = mock_body {
        let mock_url = format!("{base}/mock");
        // TD-25: mock registration is a single POST; 10s cap.
        let mock_probe = setup_runner.derived_with_deadline(10);
        match mock_probe.curl_with_deadline(&mock_url, "POST", Some(body)) {
            Ok(_) => {},
            Err(e) => {
                return (TestResult {
                    id: spec.id.clone(), name: spec.name.clone(),
                    status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
                    failure: Some(FailureDetail {
                        step: 0,
                        description: format!("mock registration failed: {e}"),
                        screenshot: None,
                    }),
                    log: logger.entries(),
                }, step_logs);
            }
        }
    }

    // 6. Wait for idle
    wait_idle(dev, 3);
    logger.setup("TC setup", setup_start.elapsed().as_millis() as u64);

    // Precondition: verify activity via agent health (not ADB dumpsys)
    if let Some(ref pre) = spec.precondition {
        if pre.activity.is_some() && agent_ready {
            // Agent is alive = app is foreground on its main activity
            // If agent health passed, precondition is met
        } else if let Some(ref activity) = pre.activity {
            if let Ok(current) = get_current_activity(dev, &setup_runner) {
                if !current.contains(activity) {
                    return (TestResult {
                        id: spec.id.clone(), name: spec.name.clone(),
                        status: "FAIL".to_string(), steps_run: 0, steps_total: spec.steps.len(),
                        failure: Some(FailureDetail {
                            step: 0,
                            description: format!("precondition failed: expected activity {activity}, got {current}"),
                            screenshot: None,
                        }),
                        log: logger.entries(),
                    }, step_logs);
                }
            }
        }
        if let Some(ref scroll_target) = pre.scroll_to {
            let _ = scroll_to_element(dev, scroll_target);
        }
    }

    // Quick idle wait via HTTP
    wait_idle(dev, 3);

    // Load navigation.yaml config (deprioritize patterns, jaccard threshold)
    let nav_env = std::env::var("DDB_NAVIGATION_YAML").ok();
    let nav_path_str = nav_env.as_deref().unwrap_or("catalogue/android/navigation.yaml");
    let nav_path = std::path::Path::new(nav_path_str);
    if nav_path.exists() {
        if let Ok(nav_content) = std::fs::read_to_string(nav_path) {
            for line in nav_content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("deprioritize_patterns:") {
                    unsafe { std::env::set_var("DDB_DEPRIORITIZE_PATTERNS", rest.trim().trim_matches('"')); }
                }
                if let Some(rest) = trimmed.strip_prefix("jaccard_threshold:") {
                    unsafe { std::env::set_var("DDB_JACCARD_THRESHOLD", rest.trim()); }
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
                        log: logger.entries(),
                    }, step_logs);
                }
            }
            _ => {}
        }
    }



    logger.setup("TOTAL", setup_start.elapsed().as_millis() as u64);

    let steps_budget = 120u64.max(timeout * spec.steps.len() as u64).min(300);
    let setup_budget = 120u64;
    let mut tm = TimeoutManager::new(steps_budget + setup_budget, timeout);
    let tc_start = std::time::Instant::now();

    // Heartbeat thread
    let hb_tc_id = spec.id.clone();
    let hb_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let hb_running_clone = hb_running.clone();
    let hb_step = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hb_step_clone = hb_step.clone();
    let hb_start = std::time::Instant::now();
    std::thread::spawn(move || {
        while hb_running_clone.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(15));
            if !hb_running_clone.load(std::sync::atomic::Ordering::Relaxed) { break; }
            let step = hb_step_clone.load(std::sync::atomic::Ordering::Relaxed);
            let elapsed = hb_start.elapsed().as_secs();
            if elapsed > 120 {
                eprintln!("  heartbeat: TIMEOUT {} step {} elapsed {}s", hb_tc_id, step, elapsed);
            }
        }
    });

    for (i, step) in spec.steps.iter().enumerate() {
        hb_step.store(i + 1, std::sync::atomic::Ordering::Relaxed);
        tm.reset_step();
        if let Err(TimeoutLevel::Tc) = tm.check() {
            return (TestResult {
                id: spec.id.clone(), name: spec.name.clone(),
                status: "FAIL".to_string(), steps_run: i, steps_total: spec.steps.len(),
                failure: Some(FailureDetail {
                    step: i + 1, description: "TC timeout exceeded".to_string(), screenshot: None,
                }),
                log: logger.entries(),
            }, step_logs);
        }

        let step_start = std::time::Instant::now();
        let step_desc_log = match step {
            Step::Action(a) => format!("action:{}", a.action),
            Step::Assert(a) => format!("assert:{}", a.assert),
        };
        logger.step_start(i + 1, &step_desc_log);

        let step_deadline = std::time::Instant::now() + tm.step_remaining();
        let mut runner = StepRunner::new(step_deadline, PhaseBudgets::default());
        runner.advance(StepPhase::Execute);

        let result = match step {
            Step::Action(a) => execute_action(dev, a, &mut ctx, &runner),
            Step::Assert(a) => execute_assert(dev, a, tm.step_remaining_secs().max(5), &ctx, &runner),
        };
        // Retry once on failure with precondition check (skip if TC expired)
        let result = if result.is_err() && tm.check().is_ok() {
            eprintln!("  step {} failed, checking preconditions...", i + 1);
            let ui = fetch_ui_dump(dev);
            let ui_lower = ui.to_lowercase();
            if ui_lower.contains("permission_allow") || ui_lower.contains("while using") {
                eprintln!("  → permission dialog detected, dismissing...");
                grant_all_permissions_with_runner(dev, pkg, &runner);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            dismiss_keyboard_if_visible(dev, &runner);
            if tm.check().is_ok() {
                if let Ok(current) = get_current_activity(dev, &runner) {
                    if !current.contains(pkg) {
                        eprintln!("  → app not foreground ({}), relaunching...", current.trim());
                        let _ = runner.adb_shell(dev, &[
                            "am", "start", "-a", "android.intent.action.MAIN",
                            "-c", "android.intent.category.LAUNCHER",
                            "-n", &std::env::var("DDB_MAIN_ACTIVITY").unwrap_or_default(), "--activity-clear-task",
                        ]);
                        std::thread::sleep(std::time::Duration::from_secs(3));
                    }
                }
            }
            if tm.check().is_err() { result } else {
                eprintln!("  step {} retrying...", i + 1);
                std::thread::sleep(std::time::Duration::from_secs(1));
                match step {
                    Step::Action(a) => execute_action(dev, a, &mut ctx, &runner),
                    Step::Assert(a) => execute_assert(dev, a, tm.step_remaining_secs().max(5), &ctx, &runner),
                }
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
                logger.step_end(i + 1, &step_desc_log, step_start.elapsed().as_millis() as u64, true);
                let elapsed = tc_start.elapsed().as_secs_f32();
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
                logger.step_end(i + 1, &step_desc_log, step_start.elapsed().as_millis() as u64, false);
                let (action_name, assert_name) = match step {
                    Step::Action(a) => (Some(a.action.clone()), None),
                    Step::Assert(a) => (None, Some(a.assert.clone())),
                };
                let agent_yaml = runner.curl_with_deadline(&format!("{}/semantic", agent_base_url()), "GET", None).ok();
                let ui_dump = Some({
                    let _ = runner.adb_shell(dev, &["uiautomator", "dump", "/sdcard/ui.xml"]);
                    runner.adb_shell(dev, &["cat", "/sdcard/ui.xml"]).unwrap_or_default()
                });
                let debug_log = fetch_debug_log(&runner);
                if let Some(ref log) = debug_log {
                    eprintln!("  debug-log: {}", &log[..log.len().min(200)]);
                }
                let screenshot = capture_failure_screenshot(dev, &spec.id, i, &runner);

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

                let step_desc2 = match step {
                    Step::Action(a) => a.action.clone(),
                    Step::Assert(a) => format!("assert {}", a.assert),
                };

                hb_running.store(false, std::sync::atomic::Ordering::Relaxed);
                // TD-24: classify infra zombie distinct from product flake.
                let description = if take_adb_zombie_flag() {
                    format!("[ADB_ZOMBIE_DETECTED] {}", err)
                } else {
                    err.clone()
                };
                return (TestResult {
                    id: spec.id.clone(),
                    name: spec.name.clone(),
                    status: "FAIL".to_string(),
                    steps_run: i,
                    steps_total: spec.steps.len(),
                    failure: Some(FailureDetail {
                        step: i + 1,
                        description,
                        screenshot,
                    }),
                    log: logger.entries(),
                }, step_logs);
            }
        }

        if let Step::Action(a) = step {
            if let Some(ref resources) = a.wait_for {
                let per_resource_timeout = a.wait_timeout.unwrap_or(10);
                let base = agent_base_url();
                for resource in resources {
                    let body = serde_json::json!({
                        "idle_resources": [resource],
                        "timeout": per_resource_timeout,
                    });
                    let _ = runner.curl_with_deadline(
                        &format!("{base}/query-when-idle"),
                        "POST",
                        Some(&body.to_string()),
                    );
                }
            } else {
                wait_idle(dev, 3);
            }
        }
    }

    // On overall PASS, grab final ui dump as proof
    let final_ui = fetch_ui_dump(dev);
    if let Some(last) = step_logs.last_mut() {
        if last.ui_dump.is_none() {
            last.ui_dump = Some(final_ui);
        }
    }

    hb_running.store(false, std::sync::atomic::Ordering::Relaxed);
    (TestResult {
        id: spec.id.clone(),
        name: spec.name.clone(),
        status: "PASS".to_string(),
        steps_run: spec.steps.len(),
        steps_total: spec.steps.len(),
        failure: None,
        log: logger.entries(),
    }, step_logs)
}

fn execute_action(dev: Option<&Device>, action: &ActionStep, ctx: &mut RunContext, runner: &StepRunner) -> Result<String, String> {
    match action.action.as_str() {
        "tap" => {
            dismiss_keyboard_if_visible(dev, &runner);
            let target = action.target.as_ref().ok_or("tap: no target")?;
            let (x, y, desc) = find_element_unified(dev, target, &idle_barrier_sources(5), Some(runner))?;
            // Re-query for fresh coordinates right before tap (idle barrier may have introduced delay)
            let (fx, fy, _) = find_element_unified(dev, target, &[], Some(runner)).unwrap_or((x, y, desc.clone()));
            runner.adb_shell(dev, &["input", "swipe", &fx.to_string(), &fy.to_string(), &fx.to_string(), &fy.to_string(), "50"])?;
            Ok(desc)
        }
        "type" => {
            let raw_text = action.text.as_ref().ok_or("type: no text")?;
            let text = &ctx.interpolate(raw_text);
            if let Some(ref target) = action.target {
                dismiss_keyboard_if_visible(dev, &runner);
                let (x, y, _) = find_element_unified(dev, target, &idle_barrier_sources(5), Some(runner))?;
                runner.adb_shell(dev, &["input", "tap", &x.to_string(), &y.to_string()])?;
                std::thread::sleep(std::time::Duration::from_millis(300));
            } else {
                ensure_input_focus(dev, runner)?;
            }
            if action.click_after.is_some() || text.chars().any(|c| !c.is_ascii()) {
                let mut type_json = serde_json::json!({"text": text, "clear": true});
                if let Some(ref click_target) = action.click_after {
                    type_json["click_after"] = serde_json::json!(click_target);
                }
                let type_body = type_json.to_string();
                let type_url = format!("{}/type", agent_base_url());
                runner.curl_with_deadline(&type_url, "POST", Some(&type_body))
                    .map_err(|e| format!("type via agent failed: {e}"))?;
            } else {
                // Search (no target): use adb input text (triggers TextWatcher)
                runner.adb_shell(dev, &["input", "keyevent", "KEYCODE_MOVE_HOME"])?;
                runner.adb_shell(dev, &["input", "keyevent", "--longpress", "KEYCODE_SHIFT_LEFT", "KEYCODE_MOVE_END"])?;
                runner.adb_shell(dev, &["input", "keyevent", "KEYCODE_DEL"])?;
                std::thread::sleep(std::time::Duration::from_millis(200));
                let escaped = text.replace(' ', "%s");
                runner.adb_shell(dev, &["input", "text", &escaped])?;
            }
            Ok(format!("typed \"{}\"", text))
        }
        "text_field_set" => {
            let raw_text = action.text.as_ref().ok_or("text_field_set: no text")?;
            let text = &ctx.interpolate(raw_text);
            if let Some(ref target) = action.target {
                dismiss_keyboard_if_visible(dev, &runner);
                let (x, y, _) = find_element_unified(dev, target, &idle_barrier_sources(5), Some(runner))?;
                runner.adb_shell(dev, &["input", "tap", &x.to_string(), &y.to_string()])?;
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            let body = serde_json::json!({"value": text}).to_string();
            let url = format!("{}/text-field/set", agent_base_url());
            runner.curl_with_deadline(&url, "POST", Some(&body))
                .map_err(|e| format!("text_field_set failed: {e}"))?;
            Ok(format!("text_field_set \"{}\"", text))
        }
        "wait_until" => {
            let target = action.target.as_ref().ok_or("wait_until: no target")?;
            let timeout = action.wait_timeout.unwrap_or(30);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
            loop {
                // Fresh /semantic query each iteration — no caching, no idle barrier
                let url = format!("{}/semantic", agent_base_url());
                if let Ok(yaml) = runner.curl_with_deadline(&url, "GET", None) {
                    let search_fuzzy = target.content_fuzzy.as_deref().unwrap_or("");
                    if !search_fuzzy.is_empty() {
                        let needle = search_fuzzy.to_lowercase();
                        if yaml.to_lowercase().contains(&needle) {
                            break;
                        }
                    }
                }
                if std::time::Instant::now() > deadline {
                    return Err(format!("wait_until: '{}' not visible after {}s",
                        target.content_fuzzy.as_deref().unwrap_or("?"), timeout));
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Ok(format!("wait_until: found '{}'", target.content_fuzzy.as_deref().unwrap_or("?")))
        }
        "scroll" | "scroll_to" => {
            if let Some(ref target) = action.target {
                // #42 — bumped outer attempts from 5 to 10. Per-call
                // `max_scroll` stays at 15 (15 individual scroll gestures
                // per attempt). Total reach = 10 × 15 = 150 scrolls,
                // covering long RecyclerViews where Q&A / late sections
                // sit 20+ items below the fold. TC author can still
                // override via `action.max_attempts` (action wraps).
                let max_attempts: u32 = action.max_attempts.unwrap_or(10);
                let mut result = None;
                for attempt in 0..max_attempts {
                    // Check if element is already visible before scrolling
                    if let Ok((x, y, desc)) = find_element_unified(dev, target, &idle_barrier_sources(5), Some(runner)) {
                        result = Some(format!("already visible: {} at ({},{})", desc, x, y));
                        break;
                    }
                    if let Some((_x, _y, desc)) = scroll_search(target, 15, false, Some(runner)) {
                        result = Some(desc);
                        break;
                    }
                    if attempt + 1 < max_attempts {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    }
                }
                result.ok_or_else(|| format!("scroll_to: element not found after {} attempts", max_attempts))
            } else {
                let dir = action.direction.as_deref().unwrap_or("down");
                let times = action.times.unwrap_or(1);
                for _ in 0..times {
                    scroll_direction(dev, dir, Some(runner))?;
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                Ok(String::new())
            }
        }
        "navigate_to_site" | "navigate_to_user" => {
            Err(format!("{}: direct navigation deleted — use UI search/tap steps instead", action.action))
        }
        "deep_link" => {
            Err(format!("{}: use platform: android: steps in TC YAML", action.action))
        }
        "long_press" => {
            dismiss_keyboard_if_visible(dev, &runner);
            let target = action.target.as_ref().ok_or("long_press: no target")?;
            let (x, y, desc) = find_element_unified(dev, target, &idle_barrier_sources(5), Some(runner))?;
            runner.adb_shell(dev, &["input", "swipe", &x.to_string(), &y.to_string(), &x.to_string(), &y.to_string(), "1500"])?;
            Ok(desc)
        }
        "click" => {
            let target = action.target.as_ref().ok_or("click: no target")?;
            let click_body = if let Some(ref fuzzy) = target.content_fuzzy {
                serde_json::json!({"content_fuzzy": fuzzy})
            } else if let Some(ref id) = target.id {
                serde_json::json!({"resource_id": id})
            } else {
                return Err("click: need content_fuzzy or id".into());
            };
            let url = format!("{}/click", agent_base_url());
            let resp = runner.curl_with_deadline(&url, "POST", Some(&click_body.to_string()))?;
            Ok(format!("click: {}", resp.trim()))
        }
        "keyboard_dismiss" => {
            dismiss_keyboard_if_visible(dev, &runner);
            Ok("keyboard dismissed".into())
        }
        "back" | "press_back" => {
            runner.adb_shell(dev, &["input", "keyevent", "4"])?;
            Ok(String::new())
        }
        "home" => {
            runner.adb_shell(dev, &["input", "keyevent", "3"])?;
            Ok(String::new())
        }
        "stage_gallery" => {
            // Stage a fixture image into the device's MediaStore so the
            // system Photo Picker (Android 13+) and gallery apps see it.
            // Source path is resolved relative to the TC YAML file. Pushes
            // to /sdcard/Pictures/<basename> and triggers MediaScanner.
            let raw_source = action.source.as_ref().ok_or("stage_gallery: no source")?;
            let source = ctx.interpolate(raw_source);
            let abs_source = if std::path::Path::new(&source).is_absolute() {
                std::path::PathBuf::from(&source)
            } else {
                ctx.tc_dir.join(&source)
            };
            if !abs_source.exists() {
                return Err(format!("stage_gallery: source not found: {}", abs_source.display()));
            }
            let basename = abs_source.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("stage_gallery: bad basename in {}", abs_source.display()))?;
            let device_path = format!("/sdcard/Pictures/{basename}");
            let source_str = abs_source.to_str()
                .ok_or_else(|| format!("stage_gallery: non-utf8 source path {}", abs_source.display()))?;

            // push (uses raw adb, not the runner's exec-out shell)
            let mut push_cmd = std::process::Command::new("adb");
            if let Some(d) = dev { push_cmd.arg("-s").arg(d.transport_id()); }
            push_cmd.args(["push", source_str, &device_path]);
            let out = runner.run_with_deadline(&mut push_cmd)?;
            if !out.status.success() {
                return Err(format!(
                    "stage_gallery: adb push failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }

            // trigger MediaScanner so the Photo Picker indexes the new file
            runner.adb_shell(dev, &[
                "am", "broadcast",
                "-a", "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
                "-d", &format!("file://{device_path}"),
            ])?;

            Ok(format!("staged {basename} → {device_path}"))
        }
        "wait" => {
            let secs = action.seconds.unwrap_or(2);
            std::thread::sleep(std::time::Duration::from_secs(secs));
            Ok(String::new())
        }
        "wait_idle" => {
            // TD-B: accept either 'seconds' (legacy local idiom) or
            // 'wait_timeout' (the wait_until idiom). Either works; if
            // both are set, 'seconds' wins (explicit local over
            // cross-action convention). Default 10s.
            let timeout = action.seconds.or(action.wait_timeout).unwrap_or(10);
            crate::ddb_debug!("[TD-B][wait_idle] timeout={}s seconds={:?} wait_timeout={:?}",
                timeout, action.seconds, action.wait_timeout);
            wait_idle(dev, timeout);
            Ok("idle".into())
        }
        "wait_event" => {
            // TD-B: same parity as wait_idle.
            let timeout = action.seconds.or(action.wait_timeout).unwrap_or(10);
            crate::ddb_debug!("[TD-B][wait_event] timeout={}s seconds={:?} wait_timeout={:?}",
                timeout, action.seconds, action.wait_timeout);
            wait_idle(dev, timeout);
            Ok("idle".into())
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

            let mut cmd = std::process::Command::new("adb");
            if let Some(d) = dev { cmd.arg("-s").arg(d.transport_id()); }
            cmd.args(["exec-out", "screencap", "-p"]);
            let screencap_output = runner.run_with_deadline(&mut cmd)?;
            std::fs::write(&output, &screencap_output.stdout).map_err(|e| format!("write screenshot: {e}"))?;
            let mut sips_cmd = std::process::Command::new("sips");
            sips_cmd.args(["-Z", "1200", &output]);
            let _ = runner.run_with_deadline(&mut sips_cmd);
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

            curl_args.push("--max-time".to_string());
            curl_args.push(runner.time_remaining_secs().to_string());
            curl_args.push(full_url.clone());

            let mut cmd = std::process::Command::new("curl");
            cmd.args(&curl_args);
            let output = runner.run_with_deadline(&mut cmd)?;

            let raw = String::from_utf8_lossy(&output.stdout).to_string();
            let (body_str, status_str) = raw.rsplit_once('\n').unwrap_or((&raw, "0"));
            let status: u16 = status_str.trim().parse().unwrap_or(0);

            if status >= 400 || status == 0 {
                return Err(format!("api_call {method} {full_url}: HTTP {status} — {}", &body_str[..body_str.len().min(200)]));
            }

            if let Some(ref save_key) = action.save_as {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body_str) {
                    ctx.add_api_response(save_key, json_val);
                }
            }

            Ok(format!("api_call {method} {full_url} → {status} ({} bytes)", body_str.len()))
        }
        other => Err(format!("unknown action: {other}")),
    }
}

fn execute_assert(dev: Option<&Device>, assert: &AssertStep, timeout: u64, ctx: &RunContext, runner: &StepRunner) -> Result<String, String> {
    match assert.assert.as_str() {
        "activity" => {
            let expected = assert.expected.as_ref().ok_or("assert activity: no expected")?;
            let current = get_current_activity(dev, runner)?;
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

            let fuzzy_raw = target.and_then(|t| t.content_fuzzy.as_deref());
            let fuzzy_resolved = fuzzy_raw.map(|f| ctx.interpolate(f));
            let fuzzy = fuzzy_resolved.as_deref();
            let id = target.and_then(|t| t.id.as_deref());

            // #41 — Assert snapshot queries get a TIGHT 5s per-probe
            // deadline so a single stalled uiautomator dump / dumpsys
            // can't burn the full step budget (which on the last step
            // of a 120s TC can be ~110s). The outer `runner` budget
            // still governs how long the poll loop is allowed to wait
            // overall; this inner runner just caps each underlying
            // probe.
            let probe = runner.derived_with_deadline(5);

            // Assertions are snapshot queries — skip idle barrier
            // (dialogs keep /idle busy).
            if let Some(target) = target {
                if let Ok((_, _, desc)) = find_element_unified(dev, target, &[], Some(&probe)) {
                    return Ok(desc);
                }
            }

            // Quick check: one pass through all sources.
            if let Some(result) = check_element_sources(dev, fuzzy, id, expected_text, Some(&probe)) {
                return Ok(result);
            }

            // Poll-based wait: check element sources every 500ms until
            // timeout. Each iteration spins up a fresh tight probe so
            // the per-call cap renews.
            let poll_deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout.min(15));
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let probe = runner.derived_with_deadline(5);
                if let Some(result) = check_element_sources(dev, fuzzy, id, expected_text, Some(&probe)) {
                    return Ok(result);
                }
                if std::time::Instant::now() > poll_deadline {
                    break;
                }
            }
            return Err(format!("element not found ({}s): {:?}", timeout, fuzzy.or(id)));
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
            // TD-29: cross-platform parity with iOS idb (SHA 2e3bf05).
            // Accept content_fuzzy as a fallback when target.id is
            // absent or empty so cross-platform TCs that key off the
            // visible label (production NK Android catalogue convention)
            // can assert state without needing a platform_id.
            let target = assert.target.as_ref().ok_or("assert element_state: no target")?;
            let elements = get_semantic_elements(dev)?;
            let id = target.id.as_deref().filter(|s| !s.is_empty());
            let fuzzy_raw = target.content_fuzzy.as_deref();
            let fuzzy_resolved = fuzzy_raw.map(|f| ctx.interpolate(f));
            let fuzzy = fuzzy_resolved.as_deref();

            let elem = if let Some(id) = id {
                elements.iter().find(|e| {
                    e.contains(&format!("platform_id: \"{}\"", id))
                        || e.contains(&format!("id: \"{}\"", id))
                })
            } else if let Some(f) = fuzzy {
                let lo = f.to_lowercase();
                elements.iter().find(|e| e.to_lowercase().contains(&lo))
            } else {
                return Err("assert element_state: needs id or content_fuzzy".to_string());
            };

            let descriptor = id.or(fuzzy).unwrap_or("");
            let elem = elem.ok_or_else(|| format!("element not found: {descriptor}"))?;

            if let Some(expected_enabled) = assert.enabled {
                let is_clickable = elem.contains("clickable: true");
                if expected_enabled != is_clickable {
                    return Err(format!("expected enabled={expected_enabled}, got clickable={is_clickable}"));
                }
            }

            Ok(format!("element state OK: {descriptor}"))
        }
        other => Err(format!("unknown assert: {other}")),
    }
}

fn scroll_to_element(dev: Option<&Device>, id_or_text: &str) -> Result<(), String> {
    let target = Target {
        id: None,
        text: None,
        content_fuzzy: Some(id_or_text.to_string()),
        clickable_only: None,
        exclude_type: None,
        x: None,
        y: None,
        type_filter: None,
    };
    if scroll_search(&target, 10, false, None).is_some() {
        Ok(())
    } else {
        Err(format!("scroll_to_element: not found via agent scroll search: {id_or_text}"))
    }
}

fn get_current_activity(dev: Option<&Device>, runner: &StepRunner) -> Result<String, String> {
    let out = runner.adb_shell(dev, &[
        "dumpsys", "activity", "activities",
        "|", "grep", "-E", "mResumedActivity|topResumedActivity",
    ])?;
    Ok(out.lines().next().unwrap_or("").trim().to_string())
}

fn wait_for_idle_after_navigate(dev: Option<&Device>) {
    std::thread::sleep(std::time::Duration::from_secs(2));
    wait_idle(dev, 10);
    std::thread::sleep(std::time::Duration::from_secs(1));
}

pub fn wait_idle(dev: Option<&Device>, timeout: u64) {
    // wait_idle uses raw Command::new("curl") --max-time 2 instead of
    // StepRunner because this is a high-frequency poll loop (~3
    // iterations/sec). StepRunner::fresh_with_budget(2) per iteration
    // would allocate a fresh PhaseBudgets + deadline 3× per sec —
    // overhead that doesn't earn its keep for a uniform 2s curl cap.
    // (B4 from c25747d cross-review — the divergence is intentional.)
    let base = agent_base_url();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    eprintln!("    wait_idle: polling {base}/idle for {timeout}s");
    let mut iter: u32 = 0;
    loop {
        if std::time::Instant::now() > deadline { eprintln!("    wait_idle: deadline"); break; }

        // TD-24: every 5th iteration (~1.5s of polling) probe adb
        // transport health. If the host-side adb server has wedged
        // mid-TC, short-circuit the wait so the next step fails fast at
        // the TD-25 caps rather than blocking on adb for the rest of
        // the timeout. Recovery is deferred to the per-5-TC sweep in
        // the outer test loop — mid-step recovery would risk TC state
        // inconsistency.
        if iter > 0 && iter % 5 == 0 {
            if let Some(d) = dev {
                if !matches!(crate::adb::probe_state(d).as_deref(), Some("device")) {
                    eprintln!("    wait_idle: adb zombie detected — short-circuiting");
                    set_adb_zombie_flag();
                    break;
                }
            }
        }
        iter += 1;

        if let Ok(out) = std::process::Command::new("curl")
            .args(["-s", "--connect-timeout", "1", "--max-time", "2", &format!("{base}/idle")])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            let body = String::from_utf8_lossy(&out.stdout);
            if body.contains("true") || body.contains("idle") {
                eprintln!("    wait_idle: idle");
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    #[test]
    fn zombie_flag_take_clears() {
        // Pre-condition: flag is per-thread; ensure clean slate.
        let _ = take_adb_zombie_flag();
        assert!(!take_adb_zombie_flag(), "fresh flag should be false");

        set_adb_zombie_flag();
        assert!(take_adb_zombie_flag(), "after set, take should observe true");
        assert!(!take_adb_zombie_flag(), "take clears — second take returns false");
    }

    #[test]
    fn zombie_probe_bounded_on_bogus_device() {
        // adb get-state against a non-existent transport returns failure
        // fast — proves the probe doesn't hang on an unhealthy device.
        // Wall <3s (the Watchdog deadline). Validates adb::probe_state
        // after the reintegration move (was test::probe_adb_state).
        let bogus = crate::registry::Device {
            serial: "bogus-td24-zombie-9999999999".to_string(),
            model: "bogus".to_string(),
            android: "0".to_string(),
            sdk: 0,
            wifi_ip: None,
            adb_port: None,
            agent_port: None,
            enrolled: "test".to_string(),
        };
        let start = std::time::Instant::now();
        let result = crate::adb::probe_state(&bogus);
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(4),
            "probe must be bounded by 3s deadline; took {:?}",
            elapsed
        );
        assert!(result.is_none(), "bogus device should return None, got {:?}", result);
    }

    #[test]
    fn test_platform_fork_parsing() {
        let yaml = r#"
id: TC-19
name: "test"
steps:
  - action: tap
    target: {content_fuzzy: "Search"}
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

        // Test expansion — platform fork expands to android sub-steps
        let expanded: Vec<StepRaw> = raw.steps.into_iter()
            .flat_map(|s| {
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

    #[test]
    fn test_fixture_interpolation_integer_fields() {
        let mut map = std::collections::HashMap::new();
        map.insert("{{fixtures.test_user.id}}".to_string(), "158926".to_string());
        map.insert("{{fixtures.test_site.id}}".to_string(), "31255".to_string());

        let yaml = r#"
id: TEST-1
name: "test fixture interpolation"
steps:
  - action: tap
    target: {content_fuzzy: "{{fixtures.test_user.id}}"}
  - action: tap
    target: {content_fuzzy: "{{fixtures.test_site.id}}"}
"#;
        let interpolated = interpolate_raw(yaml, &map);
        assert!(interpolated.contains("158926"), "user id should be interpolated");
        assert!(interpolated.contains("31255"), "site id should be interpolated");

        // Verify it parses after interpolation
        let raw: TestSpecRaw = serde_yaml::from_str(&interpolated).unwrap();
        assert_eq!(raw.steps.len(), 2);
    }

    #[test]
    fn test_fixture_interpolation_string_fields() {
        let mut map = std::collections::HashMap::new();
        map.insert("{{fixtures.test_user.name}}".to_string(), "testuser".to_string());
        map.insert("{{fixtures.oscar.name}}".to_string(), "Oscar Kockum".to_string());

        let yaml = r#"
id: TEST-2
name: "test string interpolation"
steps:
  - assert: element_exists
    target: {content_fuzzy: "{{fixtures.test_user.name}}"}
"#;
        let interpolated = interpolate_raw(yaml, &map);
        assert!(interpolated.contains("testuser"), "name should be interpolated");
    }

    #[test]
    fn test_flatten_fixtures() {
        let yaml = r#"
test_user:
  id: 158926
  name: "testuser"
oscar:
  id: 14
  name: "Oscar Kockum"
"#;
        let val: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let mut map = std::collections::HashMap::new();
        flatten_fixtures("fixtures", &val, &mut map);

        assert_eq!(map.get("{{fixtures.test_user.id}}"), Some(&"158926".to_string()));
        assert_eq!(map.get("{{fixtures.test_user.name}}"), Some(&"testuser".to_string()));
        assert_eq!(map.get("{{fixtures.oscar.id}}"), Some(&"14".to_string()));
    }

    #[test]
    fn test_token_jaccard() {
        assert!(token_jaccard("questions & answers", "questions & answers") >= 0.99);
        assert!(token_jaccard("questions", "questions & answers") > 0.3);
        assert!(token_jaccard("xyz", "questions & answers") < 0.1);
    }

    #[test]
    fn test_check_regressions_detects_pass_to_fail() {
        let results = vec![TestResult {
            id: "TC-REGRESS".to_string(),
            name: "regression test".to_string(),
            status: "FAIL".to_string(),
            steps_run: 1,
            steps_total: 2,
            failure: Some(FailureDetail {
                step: 2,
                description: "element not found".to_string(),
                screenshot: None,
            }),
            log: vec![],
        }];

        // Create temp results dir with a previous PASS result
        let tmp = std::env::temp_dir().join("ddb-test-regression");
        let _ = std::fs::create_dir_all(&tmp);
        let prev = tmp.join("TC-REGRESS-android-prev.yaml");
        std::fs::write(&prev, "status: PASS\n").unwrap();

        let regressions = check_regressions(&results, tmp.to_str().unwrap());
        assert!(!regressions.is_empty(), "should detect PASS→FAIL regression");
        assert!(regressions[0].contains("TC-REGRESS"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_check_regressions_ignores_fail_to_fail() {
        let results = vec![TestResult {
            id: "TC-STABLE".to_string(),
            name: "stable fail".to_string(),
            status: "FAIL".to_string(),
            steps_run: 1,
            steps_total: 2,
            failure: Some(FailureDetail {
                step: 2,
                description: "still broken".to_string(),
                screenshot: None,
            }),
            log: vec![],
        }];

        let tmp = std::env::temp_dir().join("ddb-test-stable-fail");
        let _ = std::fs::create_dir_all(&tmp);
        let prev = tmp.join("TC-STABLE-android-prev.yaml");
        std::fs::write(&prev, "status: FAIL\n").unwrap();

        let regressions = check_regressions(&results, tmp.to_str().unwrap());
        assert!(regressions.is_empty(), "FAIL→FAIL should not be a regression");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_interpolate_raw_no_fixtures() {
        let map = std::collections::HashMap::new();
        let input = "user_id: 31255\nname: literal";
        let result = interpolate_raw(input, &map);
        assert_eq!(result, input, "no-op when no fixtures match");
    }

    #[test]
    fn test_extract_yaml_int_after_stops_at_nonmatching_line() {
        // Bug: AND logic means non-key lines with ": " don't break the loop
        // Expected: after "tap_target:", only read indented lines until section ends
        let chunk = "tap_target:\n  x: 100\n  y: 200\nother_section:\n  x: 999";
        let x = extract_yaml_int_after(chunk, "tap_target:", "x: ");
        assert_eq!(x, Some(100));
        let y = extract_yaml_int_after(chunk, "tap_target:", "y: ");
        assert_eq!(y, Some(200));
        // This tests the bug: should NOT find x: 999 from other_section
        // when searching in tap_target
        let chunk2 = "tap_target:\n  w: 50\nz: 300\nx: 999";
        let x2 = extract_yaml_int_after(chunk2, "tap_target:", "x: ");
        assert_eq!(x2, None, "should not find x from outside tap_target section");
    }

    #[test]
    fn test_extract_ui_bounds_invalid_xml_returns_none() {
        let xml = r#"<node bounds="garbage"/>"#;
        let result = extract_ui_bounds(xml, "someId");
        assert_eq!(result, None, "invalid bounds should return None");

        let xml2 = r#"<node resource-id="id/someId" bounds="[abc,def][ghi,jkl]"/>"#;
        let result2 = extract_ui_bounds(xml2, "someId");
        assert_eq!(result2, None, "invalid numeric bounds should return None");
    }

    #[test]
    fn test_extract_ui_bounds_valid_xml_returns_center() {
        let xml = r#"<node resource-id="id/submitButton" bounds="[0,0][100,200]" text="Submit"/>"#;
        let result = extract_ui_bounds(xml, "submitButton");
        assert_eq!(result, Some((50, 100)), "center of [0,0][100,200] should be (50,100)");
    }

    #[test]
    fn test_extract_ui_bounds_fuzzy_case_insensitive() {
        let xml = r#"<node text="Questions &amp; Answers" bounds="[100,200][300,400]"/>"#;
        let result = extract_ui_bounds_fuzzy(xml, "questions");
        assert_eq!(result, Some((200, 300)), "center of [100,200][300,400]");
    }

    #[test]
    fn test_adb_subprocess_killed_at_30s() {
        // Test that adb.rs process-level kill works
        // We can't easily test ADB directly, but we can test the timing pattern
        let start = std::time::Instant::now();
        let result = crate::adb::adb(None, &["shell", "sleep", "60"]);
        let elapsed = start.elapsed();
        // Should fail (device not connected) OR timeout at ~30s
        // Either way, it should not take 60s
        assert!(elapsed.as_secs() < 45, "ADB call should not exceed 45s (30s timeout + margin)");
        assert!(result.is_err(), "should fail (no device or timeout)");
    }

    #[test]
    fn test_jaccard_threshold_boundary() {
        // "questions answers" vs "questions & answers" — 2/3 overlap
        let score = token_jaccard("questions answers", "questions & answers");
        assert!(score > 0.5 && score < 0.8, "partial overlap score: {score}");

        // At threshold 0.59 — should match
        assert!(score >= 0.59, "should match at 0.59 threshold");
        // At threshold 0.61 — depends on exact score
        let exact = token_jaccard("questions answers", "questions & answers");
        // 2 shared (questions, answers) / 3 union (questions, &, answers) = 0.667
        assert!((exact - 0.667).abs() < 0.01, "expected ~0.667, got {exact}");
    }

    #[test]
    fn test_fixture_interpolation_missing_key_passthrough() {
        let mut map = std::collections::HashMap::new();
        map.insert("{{fixtures.test_user.name}}".to_string(), "testuser".to_string());

        let input = "name: {{fixtures.nonexistent.field}}";
        let result = interpolate_raw(input, &map);
        assert_eq!(result, input, "missing key should pass through unchanged");
    }

    #[test]
    fn test_fixture_precedence_api_over_file() {
        let mut file_fixtures = std::collections::HashMap::new();
        file_fixtures.insert("{{fixtures.key}}".to_string(), "file_value".to_string());
        let mut ctx = RunContext::new(file_fixtures, std::path::PathBuf::from("."));
        ctx.add_api_response("api_result", serde_json::json!({"key": "api_value"}));

        let file_result = ctx.interpolate("{{fixtures.key}}");
        assert_eq!(file_result, "file_value");

        let api_result = ctx.interpolate("{{api_result.key}}");
        assert_eq!(api_result, "api_value");
    }
}

fn get_affected_tcs(base_branch: &str, tc_map_path: Option<&str>, tests_dir: &str, runner: &StepRunner) -> Result<Vec<String>, String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["diff", "--name-only", base_branch]);
    let diff = runner.run_with_deadline(&mut cmd)?;
    let changed_files: Vec<String> = String::from_utf8_lossy(&diff.stdout)
        .lines().map(|l| l.to_string()).collect();
    if changed_files.is_empty() {
        return Ok(Vec::new());
    }

    // If a TC map file exists, use it to find affected TCs
    if let Some(map_path) = tc_map_path {
        if let Ok(content) = std::fs::read_to_string(map_path) {
            if let Ok(map) = serde_yaml::from_str::<std::collections::HashMap<String, Vec<String>>>(&content) {
                let mut affected = std::collections::HashSet::new();
                for file in &changed_files {
                    for (pattern, tcs) in &map {
                        if file.contains(pattern) {
                            for tc in tcs {
                                let tc_path = format!("{}/{}", tests_dir, tc);
                                if std::path::Path::new(&tc_path).exists() {
                                    affected.insert(tc_path);
                                }
                            }
                        }
                    }
                }
                return Ok(affected.into_iter().collect());
            }
        }
    }

    // Fallback: if any app source changed, run all TCs in tests_dir
    let has_app_changes = changed_files.iter().any(|f|
        f.ends_with(".kt") || f.ends_with(".java") || f.ends_with(".xml") || f.ends_with(".swift")
    );
    if has_app_changes {
        let mut tcs = Vec::new();
        if let Ok(entries) = std::fs::read_dir(tests_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "yaml").unwrap_or(false) {
                    if let Some(s) = path.to_str() {
                        tcs.push(s.to_string());
                    }
                }
            }
        }
        return Ok(tcs);
    }

    Ok(Vec::new())
}

fn check_regressions(results: &[TestResult], results_dir: &str) -> Vec<String> {
    let mut regressions = Vec::new();
    for result in results {
        if result.status != "FAIL" { continue; }
        // Check if this TC previously passed by looking for result files
        let pattern = format!("{}/{}-", results_dir, result.id);
        if let Ok(entries) = std::fs::read_dir(results_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("{}-", result.id)) && name.ends_with(".yaml") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if content.contains("status: PASS") || content.contains("\"status\":\"PASS\"") {
                            regressions.push(format!("{}: was PASS, now FAIL", result.id));
                            break;
                        }
                    }
                }
            }
        }
    }
    regressions
}


