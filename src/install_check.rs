//! TD-26: forward-warn when the ddb binary on disk changes between
//! invocations. Detects the `cargo install` → stale-shell-hash pattern
//! where the operator's shell still caches the old inode and subsequent
//! commands hang until `hash -r`.
//!
//! Honest scope: this is a forward-warning, not a hang-breaker. The
//! warning fires on the FIRST invocation after install. If the
//! operator's shell hash already broke, the next ddb call may never
//! reach this code — but the operator now has the reflex documented.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::ddb_debug;

/// Location of the per-user mtime sentinel. Co-located with ddb config
/// (~/.config/ddb/) — no new dir creation since `ddb doctor` already
/// expects this layout.
fn sentinel_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/ddb/last-installed-mtime"))
}

/// Unix-seconds mtime of `p`, or None if the file is missing / stat
/// fails. Used to compare the current binary's mtime against the
/// stored sentinel.
fn mtime_secs(p: &Path) -> Option<i64> {
    let meta = std::fs::metadata(p).ok()?;
    let mt = meta.modified().ok()?;
    mt.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

#[derive(Debug, PartialEq, Eq)]
enum CheckOutcome {
    FirstRun,
    Unchanged,
    Changed { prev: i64, current: i64 },
    Skipped,
}

/// Compare binary mtime against sentinel; write new sentinel either
/// way; return the outcome. Pure file-IO, deterministic — testable
/// without needing a real binary or HOME dir.
fn check_against(binary: &Path, sentinel: &Path) -> CheckOutcome {
    let Some(current) = mtime_secs(binary) else {
        return CheckOutcome::Skipped;
    };
    let prev = mtime_secs(sentinel);

    if let Some(parent) = sentinel.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(sentinel, current.to_string());

    match prev {
        None => CheckOutcome::FirstRun,
        Some(p) if p == current => CheckOutcome::Unchanged,
        Some(p) => CheckOutcome::Changed { prev: p, current },
    }
}

/// Entry point called from main() before Cli::parse(). Best-effort —
/// silently no-ops if current_exe() / HOME / FS operations fail.
pub fn check_binary_mtime() {
    let Ok(binary) = std::env::current_exe() else {
        ddb_debug!("[TD-26][install] skipped reason=no-current-exe");
        return;
    };
    let Some(sentinel) = sentinel_path() else {
        ddb_debug!("[TD-26][install] skipped reason=no-home-dir");
        return;
    };

    ddb_debug!("[TD-26][install] binary={} sentinel={}", binary.display(), sentinel.display());

    match check_against(&binary, &sentinel) {
        CheckOutcome::FirstRun => {
            ddb_debug!("[TD-26][install] first-run — sentinel created");
        }
        CheckOutcome::Unchanged => {
            ddb_debug!("[TD-26][install] unchanged");
        }
        CheckOutcome::Changed { prev, current } => {
            eprintln!(
                "note: ddb binary updated since last invocation (mtime {prev} -> {current}). If your shell hangs on subsequent invocations, run: hash -r"
            );
        }
        CheckOutcome::Skipped => {
            ddb_debug!("[TD-26][install] skipped reason=stat-failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    fn tmp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ddb-td26-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("artifact")
    }

    fn touch(p: &Path) {
        let mut f = std::fs::OpenOptions::new()
            .create(true).write(true).truncate(true).open(p).unwrap();
        f.write_all(b"x").unwrap();
    }

    #[test]
    fn first_run_writes_sentinel_no_change_signal() {
        let bin = tmp_file("first-run-bin");
        let sentinel = tmp_file("first-run-sentinel");
        touch(&bin);
        let _ = std::fs::remove_file(&sentinel);

        let outcome = check_against(&bin, &sentinel);
        assert_eq!(outcome, CheckOutcome::FirstRun);
        assert!(sentinel.exists(), "sentinel should be written on first run");
    }

    #[test]
    fn unchanged_when_binary_mtime_matches() {
        let bin = tmp_file("unchanged-bin");
        let sentinel = tmp_file("unchanged-sentinel");
        touch(&bin);
        let _ = check_against(&bin, &sentinel);
        let outcome = check_against(&bin, &sentinel);
        assert_eq!(outcome, CheckOutcome::Unchanged);
    }

    #[test]
    fn changed_when_binary_mtime_advances() {
        let bin = tmp_file("changed-bin");
        let sentinel = tmp_file("changed-sentinel");
        touch(&bin);
        let _ = check_against(&bin, &sentinel);

        // Advance past the 1s FS mtime resolution.
        std::thread::sleep(Duration::from_secs(2));
        touch(&bin);

        let outcome = check_against(&bin, &sentinel);
        match outcome {
            CheckOutcome::Changed { prev, current } => {
                assert!(current > prev, "current mtime {current} should exceed prev {prev}");
            }
            other => panic!("expected Changed, got {:?}", other),
        }
    }
}
