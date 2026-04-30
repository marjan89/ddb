use std::process::{Command, Output};

use crate::registry::Device;

/// Run an adb command with optional device targeting, return stdout as String.
pub fn adb(device: Option<&Device>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("adb");
    if let Some(dev) = device {
        cmd.arg("-s").arg(dev.transport_id());
    }
    cmd.args(args);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run adb: {e}"))?;

    check_output(&output, args)
}

/// Run an adb shell command.
pub fn shell(device: Option<&Device>, args: &[&str]) -> Result<String, String> {
    let mut full_args = vec!["shell"];
    full_args.extend_from_slice(args);
    adb(device, &full_args)
}

/// Run adb and return raw bytes (for screencap).
pub fn adb_raw(device: Option<&Device>, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new("adb");
    if let Some(dev) = device {
        cmd.arg("-s").arg(dev.transport_id());
    }
    cmd.args(args);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run adb: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("adb {:?} failed: {stderr}", args.first()));
    }
    Ok(output.stdout)
}

/// List serials currently visible to adb (both USB and wireless).
pub fn connected_serials() -> Result<Vec<(String, String)>, String> {
    let out = adb(None, &["devices"])?;
    let mut result = Vec::new();
    for line in out.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('*') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        if let (Some(serial), Some(state)) = (parts.next(), parts.next()) {
            result.push((serial.to_string(), state.to_string()));
        }
    }
    Ok(result)
}

fn check_output(output: &Output, args: &[&str]) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let label = args.first().unwrap_or(&"");
        return Err(format!("adb {label} failed: {stderr}"));
    }
    Ok(stdout)
}
