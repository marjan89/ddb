// Epic K — observability hardening.
//
// 8 surfaces, all gated on `Observer.enabled` (the `--observability` flag).
// `--observability=off` collapses every emit fn to a no-op so byte-identical
// stderr is preserved for back-compat with existing CI consumers.
//
// Surfaces:
//   1. ERROR banner (ANSI-red + terminal bell, stderr)
//   2. Fail-fast on infra error (caller checks Observer.aborted flag)
//   3. WARN streamed during run (FD pressure, slow agent, slow adb)
//   4. Progress heartbeat ([PROGRESS N/T fails=X FD=Y agent_uptime=Z])
//   5. FD watchdog (libc::proc_pidinfo on macOS; lsof shell-out fallback)
//   6. Agent health ping between TCs (GET /health)
//   7. Per-step structured logging ([STEP TC-id step-i] ...)
//   8. --log-format=json (single-line JSON objects, schema below)
//
// JSON schema:
//   {"ts":"<iso>", "level":"info|warn|error", "kind":"banner|heartbeat|step|fd|agent",
//    "tc":"<id?>", "step":<usize?>, "msg":"<text>", ...kind-specific fields}

use crate::registry::Device;
use super::test_timeout::StepRunner;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---- legacy helpers (kept for callers in test.rs) -------------------------

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

// ---- Observer (Epic K) ----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat { Text, Json }

impl LogFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "text" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            other => Err(format!("invalid --log-format: {other} (expected text|json)")),
        }
    }
}

pub struct Observer {
    pub enabled: bool,
    pub format: LogFormat,
    pub fd_soft_limit: u64,
    pub fd_warn_pct: u8,
    pub fd_error_pct: u8,
    pub agent_base: String,
    pub started_at: Instant,
    pub aborted: Arc<AtomicBool>,
    pub last_fd_count: Arc<AtomicU64>,
    pub tcs_since_heartbeat: AtomicUsize,
    pub heartbeat_every: usize,
}

impl Observer {
    /// Construct from CLI flags. `enabled=false` produces a silent observer
    /// (every emit fn is a no-op) — preserves byte-identical stderr for CI.
    pub fn new(enabled: bool, format: LogFormat, agent_base: String) -> Self {
        let fd_soft_limit = probe_fd_soft_limit().unwrap_or(256);
        Self {
            enabled,
            format,
            fd_soft_limit,
            fd_warn_pct: 70,
            fd_error_pct: 90,
            agent_base,
            started_at: Instant::now(),
            aborted: Arc::new(AtomicBool::new(false)),
            last_fd_count: Arc::new(AtomicU64::new(0)),
            tcs_since_heartbeat: AtomicUsize::new(0),
            heartbeat_every: 5,
        }
    }

    pub fn silent() -> Self {
        Self::new(false, LogFormat::Text, String::new())
    }

    pub fn is_aborted(&self) -> bool { self.aborted.load(Ordering::Relaxed) }
    pub fn mark_aborted(&self) { self.aborted.store(true, Ordering::Relaxed); }

    // --- Surface 1: ERROR banner ------------------------------------------
    /// Surface 1+2: write an ERROR banner. `infra=true` flips the aborted flag
    /// so the sweep loop fails fast (TC-author asserts pass infra=false).
    pub fn error(&self, kind: &str, tc: Option<&str>, step: Option<usize>, msg: &str, infra: bool) {
        if !self.enabled { return; }
        if infra { self.mark_aborted(); }
        match self.format {
            LogFormat::Json => emit_json("error", kind, tc, step, msg, &[]),
            LogFormat::Text => {
                // ANSI red + bell. Bell terminator (\x07) makes TTYs ding.
                eprintln!("\x1b[31mERROR [{kind}] {}{}: {}\x1b[0m\x07",
                    tc.map(|t| format!("tc={t} ")).unwrap_or_default(),
                    step.map(|s| format!("step={s}")).unwrap_or_default(),
                    msg
                );
            }
        }
    }

    // --- Surface 3: WARN ---------------------------------------------------
    pub fn warn(&self, kind: &str, tc: Option<&str>, msg: &str) {
        if !self.enabled { return; }
        match self.format {
            LogFormat::Json => emit_json("warn", kind, tc, None, msg, &[]),
            LogFormat::Text => {
                eprintln!("\x1b[33mWARN [{kind}] {}{}\x1b[0m",
                    tc.map(|t| format!("tc={t} ")).unwrap_or_default(),
                    msg);
            }
        }
    }

    // --- Surface 4: progress heartbeat ------------------------------------
    /// Call between TCs. Emits a heartbeat every `heartbeat_every` TCs.
    pub fn maybe_heartbeat(&self, n: usize, total: usize, fails: usize) {
        if !self.enabled { return; }
        let prev = self.tcs_since_heartbeat.fetch_add(1, Ordering::Relaxed);
        if (prev + 1) % self.heartbeat_every != 0 && n != total { return; }
        let fd = self.last_fd_count.load(Ordering::Relaxed);
        let uptime = self.started_at.elapsed().as_secs();
        match self.format {
            LogFormat::Json => {
                let extras = [
                    ("n", n.to_string()),
                    ("total", total.to_string()),
                    ("fails", fails.to_string()),
                    ("fd", fd.to_string()),
                    ("agent_uptime", uptime.to_string()),
                ];
                emit_json("info", "heartbeat", None, None,
                    &format!("progress {n}/{total}"),
                    &extras.iter().map(|(k, v)| (*k, v.as_str())).collect::<Vec<_>>());
            }
            LogFormat::Text => {
                eprintln!("[PROGRESS {n}/{total} fails={fails} FD={fd} agent_uptime={uptime}s]");
            }
        }
    }

    // --- Surface 7: per-step structured logging ---------------------------
    pub fn step(&self, tc: &str, step: usize, action: &str, target: Option<&str>, outcome: &str, details: &str) {
        if !self.enabled { return; }
        match self.format {
            LogFormat::Json => {
                let target_s = target.unwrap_or("");
                let extras = [
                    ("action", action),
                    ("target", target_s),
                    ("outcome", outcome),
                    ("details", details),
                ];
                emit_json(
                    if outcome == "PASS" { "info" } else { "warn" },
                    "step", Some(tc), Some(step),
                    &format!("{action} -> {outcome}"),
                    &extras,
                );
            }
            LogFormat::Text => {
                let t = target.map(|s| format!(" target={s}")).unwrap_or_default();
                eprintln!("[STEP {tc} step-{step}] action={action}{t} outcome={outcome} details={details}");
            }
        }
    }

    // --- Surface 6: agent health ping --------------------------------------
    /// Returns Ok(()) on 2xx; Err with diagnostic on non-2xx / failure.
    /// Emits ERROR (infra=true) on failure — caller checks `is_aborted`.
    pub fn agent_health_ping(&self, runner: &StepRunner) -> Result<(), String> {
        if !self.enabled || self.agent_base.is_empty() { return Ok(()); }
        let url = format!("{}/health", self.agent_base);
        let mut cmd = std::process::Command::new("curl");
        cmd.args(["-s", "-o", "/dev/null", "-w", "%{http_code}",
                  "--connect-timeout", "2", "--max-time", "5", &url]);
        let started = Instant::now();
        let out = runner.run_with_deadline(&mut cmd);
        let elapsed = started.elapsed();
        match out {
            Ok(o) => {
                let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if code.starts_with('2') {
                    if elapsed > Duration::from_secs(3) {
                        self.warn("agent_slow", None,
                            &format!("/health took {}ms", elapsed.as_millis()));
                    }
                    Ok(())
                } else {
                    self.error("agent_unhealthy", None, None,
                        &format!("/health returned {code}"), true);
                    Err(format!("agent unhealthy: HTTP {code}"))
                }
            }
            Err(e) => {
                self.error("agent_unreachable", None, None,
                    &format!("/health failed: {e}"), true);
                Err(format!("agent unreachable: {e}"))
            }
        }
    }

    // --- Surface 5: FD probe (one-shot) -----------------------------------
    /// Probe current FD count, update last_fd_count, emit WARN/ERROR if
    /// over thresholds. Returns the count.
    pub fn check_fd_once(&self) -> u64 {
        if !self.enabled { return 0; }
        let count = probe_fd_count_self();
        self.last_fd_count.store(count, Ordering::Relaxed);
        let pct = (count * 100) / self.fd_soft_limit.max(1);
        if pct >= self.fd_error_pct as u64 {
            self.error("fd_leak", None, None,
                &format!("FD usage {count}/{} ({pct}%) >= {}%",
                    self.fd_soft_limit, self.fd_error_pct), true);
        } else if pct >= self.fd_warn_pct as u64 {
            self.warn("fd_pressure", None,
                &format!("FD usage {count}/{} ({pct}%)", self.fd_soft_limit));
        }
        count
    }
}

/// Surface 5: spawn FD watchdog thread. Polls every `interval_secs` seconds.
/// Returns a stop flag — set to `true` to terminate the watchdog.
pub fn spawn_fd_watchdog(obs: Arc<Observer>, interval_secs: u64) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    if !obs.enabled { return stop; }
    let stop_clone = stop.clone();
    std::thread::spawn(move || {
        while !stop_clone.load(Ordering::Relaxed) {
            obs.check_fd_once();
            // Short sleep granularity so stop signal is responsive.
            for _ in 0..interval_secs.max(1) {
                if stop_clone.load(Ordering::Relaxed) { break; }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
    stop
}

// ---- FD probe internals --------------------------------------------------

/// Probe RLIMIT_NOFILE soft limit. Returns None on failure.
pub fn probe_fd_soft_limit() -> Option<u64> {
    let mut rl = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl) };
    if rc == 0 { Some(rl.rlim_cur as u64) } else { None }
}

/// Probe current FD count for this process. macOS: uses proc_pidinfo
/// (PROC_PIDLISTFDS). Falls back to `lsof -p` shell-out on failure / non-mac.
pub fn probe_fd_count_self() -> u64 {
    #[cfg(target_os = "macos")]
    {
        if let Some(n) = proc_pidinfo_fd_count() { return n; }
    }
    lsof_fd_count().unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn proc_pidinfo_fd_count() -> Option<u64> {
    // PROC_PIDLISTFDS = 1; sizeof(proc_fdinfo) = 8 (int fd; uint32 proc_fdtype)
    const PROC_PIDLISTFDS: i32 = 1;
    const FDINFO_SIZE: i32 = 8;
    let pid = unsafe { libc::getpid() };
    // First call with null buffer to get required size.
    let needed = unsafe { proc_pidinfo_raw(pid, PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 { return None; }
    Some((needed / FDINFO_SIZE) as u64)
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    // Apple-private but stable since 10.5. Signature from
    // <sys/proc_info.h>. We only need the byte-count return, so the buffer
    // can be NULL with size 0 to query the required allocation.
    #[link_name = "proc_pidinfo"]
    fn proc_pidinfo_raw(pid: i32, flavor: i32, arg: u64, buffer: *mut libc::c_void, buffersize: i32) -> i32;
}

fn lsof_fd_count() -> Option<u64> {
    let pid = unsafe { libc::getpid() };
    let out = std::process::Command::new("lsof")
        .args(["-p", &pid.to_string()])
        .output()
        .ok()?;
    // Skip header line; each remaining line = one FD.
    let s = String::from_utf8_lossy(&out.stdout);
    let n = s.lines().count();
    if n == 0 { None } else { Some((n - 1) as u64) }
}

// ---- JSON emit ------------------------------------------------------------

fn emit_json(level: &str, kind: &str, tc: Option<&str>, step: Option<usize>, msg: &str, extras: &[(&str, &str)]) {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let mut obj = serde_json::Map::new();
    obj.insert("ts".into(), serde_json::json!(ts));
    obj.insert("level".into(), serde_json::json!(level));
    obj.insert("kind".into(), serde_json::json!(kind));
    if let Some(t) = tc { obj.insert("tc".into(), serde_json::json!(t)); }
    if let Some(s) = step { obj.insert("step".into(), serde_json::json!(s)); }
    obj.insert("msg".into(), serde_json::json!(msg));
    for (k, v) in extras { obj.insert((*k).into(), serde_json::json!(v)); }
    if let Ok(s) = serde_json::to_string(&serde_json::Value::Object(obj)) {
        eprintln!("{s}");
    }
}

// ---- Tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_format_parse() {
        assert_eq!(LogFormat::parse("text").unwrap(), LogFormat::Text);
        assert_eq!(LogFormat::parse("json").unwrap(), LogFormat::Json);
        assert!(LogFormat::parse("yaml").is_err());
    }

    #[test]
    fn silent_observer_does_not_abort() {
        let obs = Observer::silent();
        obs.error("test", None, None, "should be silent", true);
        // Even with infra=true, disabled observer must NOT mark aborted —
        // back-compat: --observability=off keeps old behavior.
        assert!(!obs.is_aborted());
    }

    #[test]
    fn enabled_observer_aborts_on_infra_error() {
        let obs = Observer::new(true, LogFormat::Text, String::new());
        assert!(!obs.is_aborted());
        obs.error("fd_leak", None, None, "test", true);
        assert!(obs.is_aborted());
    }

    #[test]
    fn enabled_observer_does_not_abort_on_non_infra() {
        let obs = Observer::new(true, LogFormat::Text, String::new());
        obs.error("tc_assertion_fail", Some("TC-1"), Some(3), "expected X got Y", false);
        assert!(!obs.is_aborted());
    }

    #[test]
    fn heartbeat_throttling() {
        let obs = Observer::new(true, LogFormat::Text, String::new());
        // The heartbeat counter increments each call; we just assert it does
        // not panic and the counter advances. (Output goes to stderr; we
        // can't easily capture in unit tests without redirecting fds.)
        for i in 0..12 {
            obs.maybe_heartbeat(i + 1, 100, 0);
        }
        assert!(obs.tcs_since_heartbeat.load(Ordering::Relaxed) >= 12);
    }

    #[test]
    fn fd_soft_limit_is_positive() {
        let limit = probe_fd_soft_limit().expect("getrlimit failed");
        assert!(limit > 0, "RLIMIT_NOFILE soft limit should be positive");
    }

    #[test]
    fn fd_count_is_nonzero() {
        // We're inside a running test binary — must have at least stdin/out/err.
        let n = probe_fd_count_self();
        assert!(n >= 3, "FD count should include at least stdin/out/err, got {n}");
    }

    #[test]
    fn check_fd_once_updates_last_count() {
        let obs = Observer::new(true, LogFormat::Text, String::new());
        let n = obs.check_fd_once();
        assert!(n >= 3);
        assert_eq!(obs.last_fd_count.load(Ordering::Relaxed), n);
    }

    #[test]
    fn watchdog_stop_flag_terminates_thread() {
        let obs = Arc::new(Observer::new(true, LogFormat::Text, String::new()));
        let stop = spawn_fd_watchdog(obs.clone(), 1);
        std::thread::sleep(Duration::from_millis(200));
        stop.store(true, Ordering::Relaxed);
        // No assertion beyond "doesn't deadlock" — the thread is detached.
    }

    #[test]
    fn step_emit_does_not_panic() {
        let obs = Observer::new(true, LogFormat::Json, String::new());
        obs.step("TC-22", 5, "tap", Some("login_button"), "PASS", "found at (100,200)");
        let obs2 = Observer::new(true, LogFormat::Text, String::new());
        obs2.step("TC-22", 5, "tap", None, "FAIL", "element not found");
    }

    #[test]
    fn json_emit_does_not_panic() {
        emit_json("info", "test", Some("TC-1"), Some(2), "hello", &[("k", "v")]);
    }
}
