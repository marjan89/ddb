//! ddb calibrate — runs each TC end-to-end via the shared test runner and
//! produces a calibration report that mirrors test results per step.
//!
//! Previously calibrate was static: it queried `/semantic` on the current
//! screen and tried to match each step's target text. That worked for the
//! first screen only — every multi-screen TC reported NOT_FOUND past step 2.
//!
//! Now calibrate delegates to `cmd::test::run` for each TC and reformats
//! the resulting JSON into a calibration shape. One execution path for tests
//! and calibration — same elements, same idle waits, same navigation.

use std::path::PathBuf;

use crate::cmd::test;

#[derive(clap::Args)]
pub struct CalibrateArgs {
    /// TC directory to calibrate
    #[arg(long)]
    pub tc_dir: Option<String>,

    /// Single TC file (overrides --tc-dir if set)
    #[arg(long)]
    pub spec: Option<String>,

    /// Output directory for calibration reports
    #[arg(long, default_value = "catalogue/proven/android")]
    pub output: String,

    /// Per-step timeout in seconds (passed to the test runner)
    #[arg(long, default_value = "10")]
    pub step_timeout: u64,
}

#[derive(serde::Serialize)]
struct CalibrationReport {
    tc_id: String,
    tc_name: String,
    status: String,
    steps: Vec<StepReport>,
    total: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
}

#[derive(serde::Serialize)]
struct StepReport {
    step: usize,
    action: Option<String>,
    assert: Option<String>,
    target: Option<String>,
    status: String,
    element_found: Option<String>,
    error: Option<String>,
}

fn collect_tc_files(args: &CalibrateArgs) -> Result<Vec<String>, String> {
    if let Some(ref spec) = args.spec {
        return Ok(vec![spec.clone()]);
    }
    let tc_dir = args.tc_dir.as_deref().unwrap_or("catalogue/generated/android");
    let mut out = Vec::new();
    let entries = std::fs::read_dir(tc_dir).map_err(|e| format!("read_dir {tc_dir}: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
            out.push(path.to_string_lossy().to_string());
        }
    }
    out.sort();
    Ok(out)
}

fn map_step_status(result: &str) -> &'static str {
    match result.to_lowercase().as_str() {
        "pass" => "PASS",
        "fail" => "FAIL",
        "skip" => "SKIP",
        _ => "UNKNOWN",
    }
}

fn build_calibration_report(tc_name: &str, tc_results_json: &str) -> Result<CalibrationReport, String> {
    let value: serde_json::Value = serde_json::from_str(tc_results_json)
        .map_err(|e| format!("parse test report: {e}"))?;
    // test::run writes Vec<TestResult>. Single-spec invocation → 1 element.
    let tc = value.as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| "test report empty".to_string())?;

    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or(tc_name).to_string();
    let name = tc.get("name").and_then(|v| v.as_str()).unwrap_or(tc_name).to_string();
    let log = tc.get("log").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let mut steps = Vec::with_capacity(log.len());
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    for entry in &log {
        let result = entry.get("result").and_then(|v| v.as_str()).unwrap_or("unknown");
        let status = map_step_status(result);
        match status {
            "PASS" => passed += 1,
            "FAIL" => failed += 1,
            "SKIP" => skipped += 1,
            _ => {}
        }
        steps.push(StepReport {
            step: entry.get("step").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            action: entry.get("action").and_then(|v| v.as_str()).map(String::from),
            assert: entry.get("assert").and_then(|v| v.as_str()).map(String::from),
            target: entry.get("target").and_then(|v| v.as_str()).map(String::from),
            status: status.to_string(),
            element_found: entry.get("element_found").and_then(|v| v.as_str()).map(String::from),
            error: entry.get("error").and_then(|v| v.as_str()).map(String::from),
        });
    }
    let total = steps.len();
    let status = if failed > 0 { "HAS_FAILURES" } else { "CALIBRATED" };

    Ok(CalibrationReport {
        tc_id: id,
        tc_name: name,
        status: status.into(),
        steps,
        total,
        passed,
        failed,
        skipped,
    })
}

pub fn run(dev_name: Option<&str>, args: CalibrateArgs) -> Result<(), String> {
    let out_dir = PathBuf::from(&args.output);
    std::fs::create_dir_all(out_dir.join("reports"))
        .map_err(|e| format!("mkdir reports: {e}"))?;

    let tc_files = collect_tc_files(&args)?;
    eprintln!("Calibrating {} TC(s)", tc_files.len());

    let mut all_ok = true;
    for tc_path in &tc_files {
        let tc_name = std::path::Path::new(tc_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        eprintln!("\n  Calibrating: {tc_name}");

        let tmp_report = std::env::temp_dir().join(format!("ddb-calibrate-{tc_name}.json"));
        let test_args = test::TestArgs {
            specs: vec![tc_path.clone()],
            report: Some(tmp_report.to_string_lossy().to_string()),
            step_timeout: args.step_timeout,
            rerun_failed: false,
            suite: None,
            results_dir: None,
            tests_dir: None,
            expected_hash: None,
            build: false,
            project_dir: None,
            regression_gate: false,
            base_branch: "main".into(),
            tc_map: None,
            capture_baseline: false,
            observability: "off".into(),
            log_format: "text".into(),
        };

        if let Err(e) = test::run(dev_name, test_args) {
            eprintln!("    test runner errored: {e}");
            all_ok = false;
        }

        let report_json = std::fs::read_to_string(&tmp_report)
            .map_err(|e| format!("read test report {}: {e}", tmp_report.display()))?;
        let report = build_calibration_report(&tc_name, &report_json)?;

        for step in &report.steps {
            let descr = step.action.as_deref()
                .or(step.assert.as_deref())
                .unwrap_or("?");
            eprintln!("    step {} ({}): {}", step.step, descr, step.status);
        }
        eprintln!(
            "    {} → {} ({}/{} passed, {} failed, {} skipped)",
            tc_name, report.status, report.passed, report.total, report.failed, report.skipped
        );

        if report.failed > 0 { all_ok = false; }

        let report_path = out_dir.join("reports").join(format!("{tc_name}.json"));
        std::fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap_or_default())
            .map_err(|e| format!("write report {}: {e}", report_path.display()))?;
        eprintln!("    report: {}", report_path.display());
    }

    if !all_ok { return Err("one or more TCs had failures".into()); }
    Ok(())
}
