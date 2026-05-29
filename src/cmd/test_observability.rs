use crate::adb;
use crate::registry::Device;


pub fn capture_failure_screenshot(dev: Option<&Device>, test_id: &str, step: usize) -> Option<String> {
    let filename = format!("/tmp/ddb-fail-{}-step{}.png", test_id, step);
    if adb::adb_raw(dev, &["exec-out", "screencap", "-p"]).ok()
        .and_then(|data| std::fs::write(&filename, &data).ok()).is_some()
    {
        let _ = std::process::Command::new("sips")
            .args(["--resampleWidth", "540", &filename])
            .output();
        Some(filename)
    } else {
        None
    }
}

pub fn fetch_debug_log() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--connect-timeout", "2", &format!("{}/debug-log", super::test_element::agent_base_url())])
        .output()
        .ok()?;
    let body = String::from_utf8_lossy(&output.stdout).to_string();
    if body.is_empty() { None } else { Some(body) }
}
