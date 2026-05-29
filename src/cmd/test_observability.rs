use crate::registry::Device;
use super::test_timeout::StepRunner;

pub fn capture_failure_screenshot(dev: Option<&Device>, test_id: &str, step: usize, runner: &StepRunner) -> Option<String> {
    let filename = format!("/tmp/ddb-fail-{}-step{}.png", test_id, step);
    let mut cmd = std::process::Command::new("adb");
    if let Some(d) = dev { cmd.arg("-s").arg(d.transport_id()); }
    cmd.args(["exec-out", "screencap", "-p"]);
    if let Ok(output) = runner.run_with_deadline(&mut cmd) {
        if std::fs::write(&filename, &output.stdout).is_ok() {
            let mut sips_cmd = std::process::Command::new("sips");
            sips_cmd.args(["--resampleWidth", "540", &filename]);
            let _ = runner.run_with_deadline(&mut sips_cmd);
            return Some(filename);
        }
    }
    None
}

pub fn fetch_debug_log(runner: &StepRunner) -> Option<String> {
    let body = runner.curl_with_deadline(
        &format!("{}/debug-log", super::test_element::agent_base_url()),
        "GET", None
    ).ok()?;
    if body.is_empty() { None } else { Some(body) }
}
