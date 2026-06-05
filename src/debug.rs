//! Verbose-logging gate.
//!
//! Set `DDB_DEBUG=1` (or `DDB_DEBUG=true`) to enable high-volume
//! diagnostic logs. Low-volume / high-signal logs (warnings, branch
//! decisions, errors) are emitted unconditionally via plain `eprintln!`.
//!
//! Use `ddb_debug!` for per-call / per-iteration noise that would
//! otherwise drown the steady-state output (watchdog arm/release,
//! per-probe iterations, mtime-unchanged fast path, etc.).
//!
//! Convention for log content: `[TD-<NN>][<area>] <verb> [key=val ...]`.
//! Grep-friendly + LLM-readable.

use std::sync::OnceLock;

/// Cached at first invocation — DDB_DEBUG is process-lifetime so the
/// env var only needs to be read once. Prior implementation called
/// getenv() on every `ddb_debug!` site even under DDB_DEBUG=0
/// (~10-100ns × N adb calls per TC = measurable overhead on hot
/// paths). Cross-review catch from substrate-41c9.
static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn debug_enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var("DDB_DEBUG")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
    })
}

#[macro_export]
macro_rules! ddb_debug {
    ($($arg:tt)*) => {
        if $crate::debug::debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}
