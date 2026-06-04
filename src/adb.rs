//! Host-side adb wrapper.
//!
//! Bounds every adb invocation with a watchdog from `crate::subprocess`
//! so a wedged host adb daemon cannot block ddb indefinitely. Default
//! 30s; override via `DDB_ADB_TIMEOUT`.

use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::registry::Device;
use crate::subprocess::Watchdog;
use crate::ddb_debug;

fn adb_timeout() -> Duration {
    Duration::from_secs(
        std::env::var("DDB_ADB_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
    )
}

/// Run an adb command with optional device targeting, return stdout as
/// `String`. SIGKILLs the subprocess if it exceeds `DDB_ADB_TIMEOUT`.
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
    let child = cmd.spawn().map_err(|e| format!("failed to run adb: {e}"))?;
    let pid = child.id();
    ddb_debug!("[adb] spawn label={} pid={} timeout_s={}", label, pid, timeout.as_secs());
    let _wd = Watchdog::arm(pid, timeout);
    let output = child.wait_with_output().map_err(|e| format!("adb wait error: {e}"))?;
    // _wd dropped here — disarm + join, no blocking on the timeout.

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
    let timeout = adb_timeout();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| format!("failed to run adb: {e}"))?;
    let pid = child.id();
    ddb_debug!("[adb_raw] spawn label={} pid={} timeout_s={}", label, pid, timeout.as_secs());
    let _wd = Watchdog::arm(pid, timeout);
    let output = child.wait_with_output().map_err(|e| format!("adb wait error: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal() == Some(9) {
            return Err(format!("adb {label} timed out ({}s)", timeout.as_secs()));
        }
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("adb {label} failed: {stderr}"));
    }
    Ok(output.stdout)
}

/// Detect-only probe of host-side adb (no recovery). Bounded at 3s
/// via the shared Watchdog primitive. Returns the adb get-state
/// string ("device" on healthy) or None when the transport is wedged,
/// errored, or empty.
///
/// Reintegration: moved here from test.rs (was test::probe_adb_state)
/// to live alongside other adb primitives. Used by
/// recover_adb_if_zombie (between-TC sweep) and wait_idle (mid-TC
/// detect, per TD-24).
pub fn probe_state(dev: &Device) -> Option<String> {
    let mut cmd = Command::new("adb");
    cmd.arg("-s").arg(dev.transport_id()).arg("get-state");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().ok()?;
    let pid = child.id();
    ddb_debug!("[TD-24][probe] adb get-state pid={} dev={}", pid, dev.transport_id());
    let _wd = Watchdog::arm(pid, Duration::from_secs(3));
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        ddb_debug!("[TD-24][probe] adb get-state result=non-zero-exit");
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        ddb_debug!("[TD-24][probe] adb get-state result=empty");
        None
    } else {
        ddb_debug!("[TD-24][probe] adb get-state result={}", s);
        Some(s)
    }
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
