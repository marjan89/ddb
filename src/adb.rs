use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Condvar, Mutex};
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

/// Spawn a watchdog that kills `pid` if it isn't signalled done within
/// `timeout`. Returns a Done handle; call `notify()` after the subprocess
/// completes to wake the watchdog so the JoinHandle returns immediately.
/// Without this, the prior implementation slept the full timeout in every
/// adb call before main thread could continue (60-90s overhead per CLI
/// invocation that did 2-3 adb calls).
struct DoneSignal {
    flag: Mutex<bool>,
    cvar: Condvar,
}

impl DoneSignal {
    fn new() -> Arc<Self> {
        Arc::new(Self { flag: Mutex::new(false), cvar: Condvar::new() })
    }

    fn notify(&self) {
        *self.flag.lock().unwrap() = true;
        self.cvar.notify_all();
    }
}

fn spawn_watchdog(pid: u32, timeout: Duration, done: Arc<DoneSignal>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut guard = done.flag.lock().unwrap();
        loop {
            if *guard {
                return;
            }
            let (g, res) = done.cvar.wait_timeout(guard, timeout).unwrap();
            guard = g;
            if *guard {
                return;
            }
            if res.timed_out() {
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
                return;
            }
        }
    })
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
    let done = DoneSignal::new();
    let timer = spawn_watchdog(pid, timeout, done.clone());

    let output = child.wait_with_output().map_err(|e| format!("adb wait error: {e}"))?;
    done.notify();
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
    let done = DoneSignal::new();
    let timer = spawn_watchdog(pid, adb_timeout(), done.clone());
    let output = child.wait_with_output().map_err(|e| format!("adb wait error: {e}"))?;
    done.notify();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn watchdog_join_returns_immediately_after_notify() {
        // Subprocess sleeps 200ms; watchdog timeout is 30s. Total wall
        // must be ~200ms (subprocess time), not 30s (timeout). Pre-fix:
        // timer.join() blocked the full 30s sleep regardless.
        let mut cmd = Command::new("sleep");
        cmd.arg("0.2");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let start = Instant::now();
        let mut child = cmd.spawn().expect("spawn sleep");
        let pid = child.id();
        let done = DoneSignal::new();
        let timer = spawn_watchdog(pid, Duration::from_secs(30), done.clone());
        let _ = child.wait_with_output().expect("wait sleep");
        done.notify();
        timer.join().expect("join watchdog");
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(800),
            "watchdog held the main thread for {:?} after notify (expected <800ms; subprocess was 200ms)",
            elapsed
        );
    }

    #[test]
    fn watchdog_kills_hung_subprocess_within_timeout() {
        // Subprocess sleeps 10s; watchdog timeout 300ms. Must be killed
        // by SIGKILL within ~400ms.
        let mut cmd = Command::new("sleep");
        cmd.arg("10");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let start = Instant::now();
        let mut child = cmd.spawn().expect("spawn sleep");
        let pid = child.id();
        let done = DoneSignal::new();
        let timer = spawn_watchdog(pid, Duration::from_millis(300), done.clone());
        let output = child.wait_with_output().expect("wait sleep");
        done.notify();
        timer.join().expect("join watchdog");
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(1500),
            "watchdog took {:?} to kill hung subprocess (expected <1500ms)",
            elapsed
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(output.status.signal(), Some(9), "subprocess should have been SIGKILL'd");
        }
    }
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
