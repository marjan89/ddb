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

pub struct SubprocessGuard {
    pid: u32,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SubprocessGuard {
    pub fn arm(pid: u32, timeout: Duration) -> Self {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done2 = done.clone();
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            if !done2.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .output();
            }
        });
        Self { pid, done }
    }

    pub fn disarm(&self) {
        self.done.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for SubprocessGuard {
    fn drop(&mut self) {
        self.disarm();
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
    fn test_subprocess_guard_disarms() {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = SubprocessGuard { pid: 999999, done: done.clone() };
        guard.disarm();
        assert!(done.load(std::sync::atomic::Ordering::Relaxed));
    }
}
