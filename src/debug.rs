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

pub fn debug_enabled() -> bool {
    std::env::var("DDB_DEBUG")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

#[macro_export]
macro_rules! ddb_debug {
    ($($arg:tt)*) => {
        if $crate::debug::debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}
