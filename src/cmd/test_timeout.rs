use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub enum TimeoutLevel {
    Step,
    Tc,
}

pub struct TimeoutManager {
    tc_start: Instant,
    tc_deadline: Instant,
    step_deadline: Instant,
    step_default: Duration,
}

impl TimeoutManager {
    pub fn new(tc_timeout_secs: u64, step_timeout_secs: u64) -> Self {
        let now = Instant::now();
        Self {
            tc_start: now,
            tc_deadline: now + Duration::from_secs(tc_timeout_secs),
            step_deadline: now + Duration::from_secs(step_timeout_secs),
            step_default: Duration::from_secs(step_timeout_secs),
        }
    }

    pub fn check(&self) -> Result<(), TimeoutLevel> {
        let now = Instant::now();
        if now > self.tc_deadline {
            return Err(TimeoutLevel::Tc);
        }
        if now > self.step_deadline {
            return Err(TimeoutLevel::Step);
        }
        Ok(())
    }

    pub fn reset_step(&mut self) {
        self.step_deadline = Instant::now() + self.step_default.min(self.remaining());
    }

    pub fn reset_step_with(&mut self, secs: u64) {
        let dur = Duration::from_secs(secs).min(self.remaining());
        self.step_deadline = Instant::now() + dur;
    }

    pub fn remaining(&self) -> Duration {
        self.tc_deadline.saturating_duration_since(Instant::now())
    }

    pub fn step_remaining(&self) -> Duration {
        let tc_rem = self.tc_deadline.saturating_duration_since(Instant::now());
        let step_rem = self.step_deadline.saturating_duration_since(Instant::now());
        tc_rem.min(step_rem)
    }

    pub fn step_remaining_secs(&self) -> u64 {
        self.step_remaining().as_secs()
    }

    pub fn tc_elapsed(&self) -> Duration {
        self.tc_start.elapsed()
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() > self.tc_deadline
    }
}

// SubprocessGuard removed — consolidated into crate::subprocess::Watchdog
// (reintegration of TD-32 watchdog primitive across adb.rs +
// test_timeout.rs). The Condvar-based Watchdog notifies on disarm so
// the timer thread exits immediately rather than sleeping the full
// timeout in the background, eliminating thread-pool pressure under
// high adb-call volume.

use crate::adb;
use crate::subprocess::Watchdog;
use crate::registry::Device;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepPhase {
    PreIdle,
    Execute,
    PostIdle,
}

pub struct PhaseBudgets {
    pub pre_idle_s: u64,
    pub execute_s: u64,
    pub post_idle_s: u64,
}

impl Default for PhaseBudgets {
    fn default() -> Self {
        Self { pre_idle_s: 3, execute_s: 30, post_idle_s: 3 }
    }
}

pub struct StepRunner {
    deadline: Instant,
    phase: StepPhase,
    budgets: PhaseBudgets,
    phase_deadline: Instant,
}

impl StepRunner {
    pub fn new(step_deadline: Instant, budgets: PhaseBudgets) -> Self {
        let now = Instant::now();
        let phase_end = now + Duration::from_secs(budgets.pre_idle_s);
        Self {
            deadline: step_deadline,
            phase: StepPhase::PreIdle,
            budgets,
            phase_deadline: phase_end.min(step_deadline),
        }
    }

    /// Construct a fresh StepRunner that ignores any outer budget — used
    /// for long-running standalone operations (gradle build, apk install,
    /// set_animations) where derived_with_deadline would clamp to the
    /// caller's smaller cap. PhaseBudgets are set uniformly to `secs`
    /// across pre_idle/execute, with a 3s post_idle settle. Consolidates
    /// the 3 inline `let d = now + ...; let r = StepRunner::new(d, ...);`
    /// sites introduced in TD-25.
    pub fn fresh_with_budget(secs: u64) -> Self {
        let deadline = Instant::now() + Duration::from_secs(secs);
        Self::new(deadline, PhaseBudgets { pre_idle_s: secs, execute_s: secs, post_idle_s: 3 })
    }

    pub fn advance(&mut self, phase: StepPhase) {
        self.phase = phase;
        let budget = match phase {
            StepPhase::PreIdle => self.budgets.pre_idle_s,
            StepPhase::Execute => self.budgets.execute_s,
            StepPhase::PostIdle => self.budgets.post_idle_s,
        };
        let phase_end = Instant::now() + Duration::from_secs(budget);
        self.phase_deadline = phase_end.min(self.deadline);
    }

    pub fn expired(&self) -> bool {
        Instant::now() > self.deadline
    }

    /// New runner with a tight deadline (max `secs` seconds from now).
    /// Used by assert snapshot-query paths that need to bound each
    /// underlying probe (adb shell uiautomator dump, dumpsys, curl) so
    /// a single stalled call can't burn the outer step budget. The
    /// returned runner runs in the Execute phase with the same secs as
    /// the phase budget, so `time_remaining()` and `run_with_deadline`
    /// both honor the cap.
    pub fn derived_with_deadline(&self, secs: u64) -> StepRunner {
        let now = Instant::now();
        let cap = Duration::from_secs(secs);
        let outer = self.deadline.saturating_duration_since(now);
        let bound = cap.min(outer);
        let new_deadline = now + bound;
        StepRunner {
            deadline: new_deadline,
            phase: StepPhase::Execute,
            budgets: PhaseBudgets { pre_idle_s: 0, execute_s: secs, post_idle_s: 0 },
            phase_deadline: new_deadline,
        }
    }

    pub fn time_remaining(&self) -> Duration {
        let step_rem = self.deadline.saturating_duration_since(Instant::now());
        let phase_rem = self.phase_deadline.saturating_duration_since(Instant::now());
        step_rem.min(phase_rem)
    }

    pub fn time_remaining_secs(&self) -> u64 {
        self.time_remaining().as_secs().max(1)
    }

    pub fn phase(&self) -> StepPhase {
        self.phase
    }

    pub fn run_with_deadline(&self, cmd: &mut Command) -> Result<Output, String> {
        let timeout = self.time_remaining();
        if timeout.is_zero() {
            return Err("step deadline exceeded before subprocess".into());
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
        let _wd = Watchdog::arm(child.id(), timeout);
        let output = child.wait_with_output().map_err(|e| format!("wait failed: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if output.status.signal() == Some(9) {
                return Err(format!("subprocess killed (deadline {}s)", timeout.as_secs()));
            }
        }
        Ok(output)
    }

    pub fn adb_shell(&self, dev: Option<&Device>, args: &[&str]) -> Result<String, String> {
        if self.expired() {
            return Err("step deadline exceeded before adb".into());
        }
        let timeout = self.time_remaining();
        let mut full_args = vec!["shell"];
        full_args.extend_from_slice(args);
        let mut cmd = Command::new("adb");
        if let Some(d) = dev {
            cmd.arg("-s").arg(d.transport_id());
        }
        cmd.args(&full_args);
        let output = self.run_with_deadline(&mut cmd)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("adb shell failed: {stderr}"));
        }
        Ok(stdout)
    }

    pub fn curl_with_deadline(&self, url: &str, method: &str, body: Option<&str>) -> Result<String, String> {
        let timeout_s = self.time_remaining_secs();
        let mut cmd = Command::new("curl");
        cmd.args(["-s", "--connect-timeout", "2", "--max-time", &timeout_s.to_string()]);
        if method == "POST" {
            cmd.args(["-X", "POST", "-H", "Content-Type: application/json"]);
            if let Some(b) = body {
                cmd.args(["-d", b]);
            }
        }
        cmd.arg(url);
        let output = self.run_with_deadline(&mut cmd)?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_manager_not_expired_initially() {
        let tm = TimeoutManager::new(120, 30);
        assert!(tm.check().is_ok());
        assert!(!tm.is_expired());
        assert!(tm.remaining().as_secs() >= 119);
    }

    #[test]
    fn test_timeout_manager_step_capped_by_tc() {
        let tm = TimeoutManager::new(5, 30);
        assert!(tm.step_remaining().as_secs() <= 5);
    }

    #[test]
    fn test_timeout_manager_reset_step() {
        let mut tm = TimeoutManager::new(120, 5);
        std::thread::sleep(Duration::from_millis(50));
        tm.reset_step();
        assert!(tm.step_remaining().as_secs() >= 4);
    }

    #[test]
    fn test_step_runner_phases() {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut runner = StepRunner::new(deadline, PhaseBudgets::default());
        assert_eq!(runner.phase(), StepPhase::PreIdle);
        assert!(!runner.expired());
        assert!(runner.time_remaining_secs() <= 3);

        runner.advance(StepPhase::Execute);
        assert_eq!(runner.phase(), StepPhase::Execute);
        assert!(runner.time_remaining_secs() <= 30);

        runner.advance(StepPhase::PostIdle);
        assert_eq!(runner.phase(), StepPhase::PostIdle);
        assert!(runner.time_remaining_secs() <= 3);
    }

    #[test]
    fn test_step_runner_deadline_caps_phase() {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut runner = StepRunner::new(deadline, PhaseBudgets { pre_idle_s: 3, execute_s: 30, post_idle_s: 3 });
        assert!(runner.time_remaining_secs() <= 2);
        runner.advance(StepPhase::Execute);
        assert!(runner.time_remaining_secs() <= 2);
    }

    #[test]
    fn test_step_runner_run_with_deadline() {
        let deadline = Instant::now() + Duration::from_secs(10);
        let runner = StepRunner::new(deadline, PhaseBudgets::default());
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        let output = runner.run_with_deadline(&mut cmd).unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[test]
    fn test_step_runner_kills_on_deadline() {
        let deadline = Instant::now() + Duration::from_secs(2);
        let runner = StepRunner::new(deadline, PhaseBudgets { pre_idle_s: 1, execute_s: 1, post_idle_s: 1 });
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let result = runner.run_with_deadline(&mut cmd);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("killed"));
    }

    // SubprocessGuard test removed — Watchdog is exercised by
    // subprocess::tests (watchdog_disarm_completes_fast_after_drop +
    // watchdog_kills_hung_subprocess). Equivalent coverage at the
    // consolidated layer.
}
