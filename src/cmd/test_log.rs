use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub enum LogLevel {
    Error = 0,
    Info = 1,
    Debug = 2,
}

impl LogLevel {
    pub fn from_env() -> Self {
        match std::env::var("DDB_LOG_LEVEL").as_deref() {
            Ok("debug") => LogLevel::Debug,
            Ok("info") => LogLevel::Info,
            _ => LogLevel::Error,
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct LogEntry {
    pub elapsed_ms: u64,
    pub level: String,
    pub category: String,
    pub message: String,
}

#[derive(Clone)]
pub struct Logger {
    level: LogLevel,
    start: Instant,
    entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl Logger {
    pub fn new() -> Self {
        Self {
            level: LogLevel::from_env(),
            start: Instant::now(),
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn log(&self, level: LogLevel, category: &str, message: String) {
        if level > self.level {
            return;
        }
        let entry = LogEntry {
            elapsed_ms: self.start.elapsed().as_millis() as u64,
            level: match level {
                LogLevel::Error => "error",
                LogLevel::Info => "info",
                LogLevel::Debug => "debug",
            }.to_string(),
            category: category.to_string(),
            message: message.clone(),
        };
        let prefix = match level {
            LogLevel::Error => "ERR",
            LogLevel::Info => "   ",
            LogLevel::Debug => "DBG",
        };
        eprintln!("[{:>6}ms] {} [{}] {}", entry.elapsed_ms, prefix, category, message);
        self.entries.lock().unwrap().push(entry);
    }

    pub fn error(&self, category: &str, message: String) {
        self.log(LogLevel::Error, category, message);
    }

    pub fn info(&self, category: &str, message: String) {
        self.log(LogLevel::Info, category, message);
    }

    pub fn debug(&self, category: &str, message: String) {
        self.log(LogLevel::Debug, category, message);
    }

    pub fn http(&self, url: &str, status: u16, duration_ms: u64) {
        self.info("http", format!("{url} → {status} ({duration_ms}ms)"));
    }

    pub fn http_detail(&self, url: &str, status: u16, duration_ms: u64, body: &str) {
        self.info("http", format!("{url} → {status} ({duration_ms}ms)"));
        self.debug("http", format!("response: {}", &body[..body.len().min(500)]));
    }

    pub fn adb(&self, args: &[&str], exit_code: i32, duration_ms: u64) {
        let cmd = args.join(" ");
        if exit_code == 0 {
            self.info("adb", format!("adb {cmd} → ok ({duration_ms}ms)"));
        } else {
            self.error("adb", format!("adb {cmd} → exit {exit_code} ({duration_ms}ms)"));
        }
    }

    pub fn step_start(&self, step_idx: usize, description: &str) {
        self.info("step", format!("[{step_idx}] START {description}"));
    }

    pub fn step_end(&self, step_idx: usize, description: &str, duration_ms: u64, ok: bool) {
        let status = if ok { "PASS" } else { "FAIL" };
        self.info("step", format!("[{step_idx}] {status} {description} ({duration_ms}ms)"));
    }

    pub fn setup(&self, phase: &str, duration_ms: u64) {
        self.info("setup", format!("{phase} ({duration_ms}ms)"));
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_logger(level: LogLevel) -> Logger {
        Logger {
            level,
            start: Instant::now(),
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[test]
    fn test_log_level_filtering() {
        let logger = make_logger(LogLevel::Info);
        logger.error("test", "error msg".into());
        logger.info("test", "info msg".into());
        logger.debug("test", "debug msg".into());
        let entries = logger.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].level, "error");
        assert_eq!(entries[1].level, "info");
    }

    #[test]
    fn test_http_log_entry() {
        let logger = make_logger(LogLevel::Info);
        logger.http("http://localhost:9876/health", 200, 42);
        let entries = logger.entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].message.contains("200"));
        assert!(entries[0].message.contains("42ms"));
        assert_eq!(entries[0].category, "http");
    }

    #[test]
    fn test_adb_log_entry() {
        let logger = make_logger(LogLevel::Info);
        logger.adb(&["shell", "pm", "list"], 0, 150);
        let entries = logger.entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].message.contains("ok"));
        assert!(entries[0].message.contains("150ms"));
    }

    #[test]
    fn test_error_level_filters_info() {
        let logger = make_logger(LogLevel::Error);
        logger.error("test", "visible".into());
        logger.info("test", "hidden".into());
        assert_eq!(logger.entries().len(), 1);
    }
}
