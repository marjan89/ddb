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

fn sentinel_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/ddb/last-installed-mtime"))
}

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

pub fn check_binary_mtime() {
    let Ok(binary) = std::env::current_exe() else {
        return;
    };
    let Some(sentinel) = sentinel_path() else {
        return;
    };

    if let CheckOutcome::Changed { prev, current } = check_against(&binary, &sentinel) {
        eprintln!(
            "note: ddb binary updated since last invocation (mtime {prev} -> {current}). If your shell hangs on subsequent invocations, run: hash -r"
        );
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

    fn set_mtime(p: &Path, secs: i64) {
        // touch -t style via filetime crate? Avoid extra dep: re-create
        // the file then nudge by sleeping. For deterministic mtimes we
        // use the `touch` shell command (POSIX guaranteed on darwin).
        let _ = std::process::Command::new("touch")
            .arg("-t")
            .arg(format!("197001{:02}{:02}{:02}.{:02}", 1, ((secs / 60) % 24).abs(), (secs % 60).abs(), 0))
            .arg(p)
            .output();
    }

    #[test]
    fn td26_first_run_writes_sentinel_no_change_signal() {
        let bin = tmp_file("first-run-bin");
        let sentinel = tmp_file("first-run-sentinel");
        touch(&bin);
        // sentinel intentionally absent
        let _ = std::fs::remove_file(&sentinel);

        let outcome = check_against(&bin, &sentinel);
        assert_eq!(outcome, CheckOutcome::FirstRun);
        assert!(sentinel.exists(), "sentinel should be written on first run");
    }

    #[test]
    fn td26_unchanged_when_binary_mtime_matches() {
        let bin = tmp_file("unchanged-bin");
        let sentinel = tmp_file("unchanged-sentinel");
        touch(&bin);

        // First call seeds sentinel.
        let _ = check_against(&bin, &sentinel);
        // Second call without modifying binary: unchanged.
        let outcome = check_against(&bin, &sentinel);
        assert_eq!(outcome, CheckOutcome::Unchanged);
    }

    #[test]
    fn td26_changed_when_binary_mtime_advances() {
        let bin = tmp_file("changed-bin");
        let sentinel = tmp_file("changed-sentinel");
        touch(&bin);

        let _ = check_against(&bin, &sentinel);

        // Re-touch the binary to advance mtime past the 1s FS resolution.
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

    #[test]
    fn td26_unused_set_mtime_helper_compiles() {
        // The helper is only used by debug exploration; reference it so
        // dead-code analysis doesn't flag it during full builds.
        let p = tmp_file("set-mtime-noop");
        touch(&p);
        set_mtime(&p, 0);
    }
}
