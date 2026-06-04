//! Shared subprocess-watchdog primitive.
//!
//! Spawns a background thread that SIGKILLs a given pid if it isn't
//! disarmed within `timeout`. Disarm wakes the thread via Condvar so
//! the watchdog's owning thread can join it immediately on success
//! (rather than blocking for the full timeout — the bug fixed by TD-32
//! in adb.rs which this module now consolidates).
//!
//! RAII shape: `arm` returns a `Watchdog`; dropping it disarms +
//! joins. Use as `let _wd = Watchdog::arm(pid, dur);` and the watchdog
//! is automatically cleaned up when the binding goes out of scope.
//!
//! Consolidates two prior mechanisms:
//! - adb.rs DoneSignal + spawn_watchdog (TD-32, explicit notify+join)
//! - test_timeout.rs SubprocessGuard (atomic+detached, RAII disarm)
//!
//! Reintegration: replaces both with a single Condvar-based RAII
//! primitive. Kills nothing on success (notify wakes the thread before
//! it sleeps the full timeout — no orphaned threads accumulate under
//! load, no fire-and-forget thread leak).

use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::ddb_debug;

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

fn spawn_thread(pid: u32, timeout: Duration, done: Arc<DoneSignal>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        ddb_debug!("[TD-32][watchdog] arm pid={} timeout_s={}", pid, timeout.as_secs());
        let mut guard = done.flag.lock().unwrap();
        loop {
            if *guard {
                ddb_debug!("[TD-32][watchdog] release pid={} reason=already-notified", pid);
                return;
            }
            let (g, res) = done.cvar.wait_timeout(guard, timeout).unwrap();
            guard = g;
            if *guard {
                ddb_debug!("[TD-32][watchdog] release pid={} reason=notified", pid);
                return;
            }
            if res.timed_out() {
                eprintln!("[TD-32][watchdog] kill pid={} reason=timeout deadline_s={}", pid, timeout.as_secs());
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
                return;
            }
        }
    })
}

/// RAII subprocess-kill watchdog. SIGKILLs `pid` after `timeout`
/// unless `disarm()` is called first (Drop calls disarm + joins the
/// thread, so the caller's wall time tracks the subprocess wall time,
/// not the timeout).
pub struct Watchdog {
    pid: u32,
    done: Arc<DoneSignal>,
    handle: Option<JoinHandle<()>>,
}

impl Watchdog {
    /// Arm the watchdog. Returns immediately; the timer runs in a
    /// background thread.
    pub fn arm(pid: u32, timeout: Duration) -> Self {
        let done = DoneSignal::new();
        let handle = spawn_thread(pid, timeout, done.clone());
        Self { pid, done, handle: Some(handle) }
    }

    /// Wake the watchdog thread so it exits without killing the
    /// subprocess. Idempotent. Drop also calls this.
    pub fn disarm(&self) {
        ddb_debug!("[TD-32][watchdog] disarm pid={}", self.pid);
        self.done.notify();
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.disarm();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::Instant;

    #[test]
    fn watchdog_disarm_completes_fast_after_drop() {
        // Sleep 200ms; watchdog timeout 30s. Total wall must be ~200ms
        // (subprocess time), not 30s. Pre-consolidation this was the
        // TD-32 bug: timer.join() blocked the full timeout.
        let mut cmd = Command::new("sleep");
        cmd.arg("0.2").stdout(Stdio::piped()).stderr(Stdio::piped());
        let start = Instant::now();
        let mut child = cmd.spawn().expect("spawn");
        {
            let _wd = Watchdog::arm(child.id(), Duration::from_secs(30));
            let _ = child.wait_with_output().expect("wait");
            // _wd dropped here — disarm + join in <100ms
        }
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(800),
            "watchdog held main thread for {:?} after drop (expected <800ms)", elapsed);
    }

    #[test]
    fn watchdog_kills_hung_subprocess() {
        let mut cmd = Command::new("sleep");
        cmd.arg("10").stdout(Stdio::piped()).stderr(Stdio::piped());
        let start = Instant::now();
        let mut child = cmd.spawn().expect("spawn");
        let _wd = Watchdog::arm(child.id(), Duration::from_millis(300));
        let output = child.wait_with_output().expect("wait");
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(1500),
            "watchdog took {:?} to kill (expected <1500ms)", elapsed);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(output.status.signal(), Some(9), "subprocess should be SIGKILL'd");
        }
    }
}
