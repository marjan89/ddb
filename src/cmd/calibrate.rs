use std::path::PathBuf;

#[derive(clap::Args)]
pub struct CalibrateArgs {
    /// TC directory to calibrate
    #[arg(long)]
    pub tc_dir: Option<String>,

    /// Single recipe file to calibrate
    #[arg(long)]
    pub recipe: Option<String>,

    /// Crawl data directory (for element verification)
    #[arg(long)]
    pub crawl_dir: Option<String>,

    /// Output directory for calibrated TCs
    #[arg(long, default_value = "catalogue/proven/android")]
    pub output: String,
}

#[derive(serde::Serialize)]
struct CalibrationReport {
    tc_id: String,
    tc_name: String,
    status: String,
    steps: Vec<StepReport>,
    total: usize,
    passed: usize,
    adjusted: usize,
    failed: usize,
}

#[derive(serde::Serialize)]
struct StepReport {
    step: usize,
    action: String,
    status: String,
    old_target: Option<String>,
    new_target: Option<String>,
    reason: Option<String>,
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

fn find_in_semantic(semantic: &str, target: &str) -> MatchResult {
    let target_lower = target.to_lowercase();
    let lines: Vec<&str> = semantic.lines().collect();

    // Exact match
    for line in &lines {
        let trimmed = line.trim().trim_start_matches("- content:").trim_start_matches("content:").trim().trim_matches('"');
        if trimmed == target { return MatchResult::Exact; }
    }

    // Case-insensitive
    for line in &lines {
        let trimmed = line.trim().trim_start_matches("- content:").trim_start_matches("content:").trim().trim_matches('"');
        if trimmed.to_lowercase() == target_lower { return MatchResult::Fuzzy(trimmed.to_string()); }
    }

    // Substring
    for line in &lines {
        let trimmed = line.trim().trim_start_matches("- content:").trim_start_matches("content:").trim().trim_matches('"');
        if !trimmed.is_empty() && trimmed.to_lowercase().contains(&target_lower) {
            return MatchResult::Fuzzy(trimmed.to_string());
        }
    }

    // Reverse substring
    for line in &lines {
        let trimmed = line.trim().trim_start_matches("- content:").trim_start_matches("content:").trim().trim_matches('"');
        if !trimmed.is_empty() && target_lower.contains(&trimmed.to_lowercase()) && trimmed.len() > 3 {
            return MatchResult::Fuzzy(trimmed.to_string());
        }
    }

    MatchResult::NotFound
}

enum MatchResult {
    Exact,
    Fuzzy(String),
    NotFound,
}

fn extract_target_text(step_yaml: &str) -> Option<String> {
    if let Some(pos) = step_yaml.find("content_fuzzy:") {
        let rest = &step_yaml[pos + 14..];
        let trimmed = rest.trim().trim_matches('"').trim_matches('\'');
        let end = trimmed.find('}').or_else(|| trimmed.find(',')).unwrap_or(trimmed.len());
        return Some(trimmed[..end].trim().trim_matches('"').trim_matches('\'').to_string());
    }
    None
}

pub fn run(dev_name: Option<&str>, args: CalibrateArgs) -> Result<(), String> {
    let base = agent_base_url();

    // Wait for agent
    let mut ready = false;
    for _ in 0..10 {
        if curl_get(&format!("{base}/health")).map(|b| b.contains("semantic-agent")).unwrap_or(false) { ready = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !ready { return Err("agent not ready".into()); }

    let tc_dir = args.tc_dir.as_deref().unwrap_or("catalogue/generated/android");
    let out_dir = PathBuf::from(&args.output);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("mkdir: {e}"))?;

    let mut tc_files: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(tc_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                tc_files.push(path.to_string_lossy().to_string());
            }
        }
    }
    tc_files.sort();

    eprintln!("Calibrating {} TCs from {}", tc_files.len(), tc_dir);

    for tc_path in &tc_files {
        let content = std::fs::read_to_string(tc_path).map_err(|e| format!("read {tc_path}: {e}"))?;
        let tc_name = std::path::Path::new(tc_path).file_stem()
            .map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

        eprintln!("\n  Calibrating: {tc_name}");

        // Parse TC YAML lines to find steps with targets
        let lines: Vec<&str> = content.lines().collect();
        let mut report = CalibrationReport {
            tc_id: tc_name.clone(), tc_name: tc_name.clone(),
            status: "CALIBRATED".into(),
            steps: Vec::new(), total: 0, passed: 0, adjusted: 0, failed: 0,
        };

        let mut calibrated = content.clone();
        let mut step_num = 0;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("- action:") || trimmed.starts_with("- assert:") {
                step_num += 1;
                report.total += 1;

                let action = trimmed.strip_prefix("- action:").or_else(|| trimmed.strip_prefix("- assert:"))
                    .unwrap_or("").trim().to_string();

                // Find the target line (next few lines)
                let step_block: String = lines[i..std::cmp::min(i + 5, lines.len())].join("\n");
                let target_text = extract_target_text(&step_block);

                if let Some(ref target) = target_text {
                    // Query current semantic
                    let semantic = curl_get(&format!("{base}/semantic")).unwrap_or_default();
                    let result = find_in_semantic(&semantic, target);

                    match result {
                        MatchResult::Exact => {
                            report.passed += 1;
                            report.steps.push(StepReport {
                                step: step_num, action: action.clone(), status: "PASS".into(),
                                old_target: None, new_target: None, reason: None,
                            });
                            eprintln!("    step {step_num} ({action}): PASS");
                        }
                        MatchResult::Fuzzy(actual) => {
                            report.adjusted += 1;
                            calibrated = calibrated.replace(target, &actual);
                            report.steps.push(StepReport {
                                step: step_num, action: action.clone(), status: "ADJUSTED".into(),
                                old_target: Some(target.clone()), new_target: Some(actual.clone()),
                                reason: Some("text mismatch — adjusted to match actual UI".into()),
                            });
                            eprintln!("    step {step_num} ({action}): ADJUSTED '{}' → '{}'", target, actual);
                        }
                        MatchResult::NotFound => {
                            report.failed += 1;
                            report.status = "HAS_FAILURES".into();
                            report.steps.push(StepReport {
                                step: step_num, action: action.clone(), status: "NOT_FOUND".into(),
                                old_target: Some(target.clone()), new_target: None,
                                reason: Some("element not found on current screen".into()),
                            });
                            eprintln!("    step {step_num} ({action}): NOT_FOUND '{}'", target);
                        }
                    }
                } else {
                    report.passed += 1;
                    report.steps.push(StepReport {
                        step: step_num, action: action.clone(), status: "PASS".into(),
                        old_target: None, new_target: None, reason: None,
                    });
                }
            }
        }

        // Write calibrated TC
        let out_path = out_dir.join(format!("{tc_name}.yaml"));
        let _ = std::fs::write(&out_path, &calibrated);

        // Write report
        let report_dir = out_dir.join("reports");
        let _ = std::fs::create_dir_all(&report_dir);
        let _ = std::fs::write(report_dir.join(format!("{tc_name}-report.yaml")),
            serde_yaml::to_string(&report).unwrap_or_default());

        eprintln!("  {tc_name}: {}/{} pass, {} adjusted, {} failed",
            report.passed, report.total, report.adjusted, report.failed);
    }

    Ok(())
}
