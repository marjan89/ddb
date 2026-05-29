use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::registry::Device;

fn adb_timeout() -> Duration {
    Duration::from_secs(
        std::env::var("DDB_ADB_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
    )
}

/// Run an adb command with optional device targeting, return stdout as String.
/// Kills the subprocess if it exceeds the ADB timeout.
pub fn adb(device: Option<&Device>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("adb");
    if let Some(dev) = device {
        cmd.arg("-s").arg(dev.transport_id());
    }
    cmd.args(args);

    let label = args.first().copied().unwrap_or("");
    run_with_timeout(cmd, adb_timeout(), label)
}

fn run_with_timeout(mut cmd: Command, timeout: Duration, label: &str) -> Result<String, String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("failed to run adb: {e}"))?;
    let pid = child.id();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done2 = done.clone();

    let timer = std::thread::spawn(move || {
        std::thread::sleep(timeout);
        if !done2.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
        }
    });

    let output = child.wait_with_output().map_err(|e| format!("adb wait error: {e}"))?;
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = timer.join();

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal() == Some(9) {
            return Err(format!("adb {label} timed out ({}s)", timeout.as_secs()));
        }
    }
    check_output(&output, &[label])
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

    let label = args.first().copied().unwrap_or("");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("failed to run adb: {e}"))?;
    let pid = child.id();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done2 = done.clone();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(adb_timeout());
        if !done2.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
        }
    });
    let output = child.wait_with_output().map_err(|e| format!("adb wait error: {e}"))?;
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = timer.join();

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal() == Some(9) {
            return Err(format!("adb {label} timed out ({}s)", adb_timeout().as_secs()));
        }
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("adb {label} failed: {stderr}"));
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
