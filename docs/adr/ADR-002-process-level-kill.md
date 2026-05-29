# ADR-002: Process-Level Kill for ADB Subprocesses

## Status: Accepted

## Context

ADB commands can hang indefinitely — device disconnects, USB timeouts, uiautomator dump on Samsung dialogs. The runner had no way to terminate a stuck ADB call. A scroll_to looking for a missing element would run 20 iterations × 10s per ADB call = 200+ seconds with the process stuck in a blocking `wait_with_output()`.

Flag-based approaches (AtomicBool checked between iterations) don't help because the current ADB call blocks the thread — the flag check only fires after the call returns.

## Decision

Wrap every ADB subprocess in a 30s kill timer:
1. `Command::spawn()` the child process
2. Background thread sleeps 30s, then `kill -9` the PID
3. Main thread calls `wait_with_output()` — returns when child exits or is killed
4. On SIGKILL (signal 9): return `Err("adb timed out (30s)")`
5. If child exits normally before 30s: timer thread cancelled via AtomicBool

Implemented in `adb.rs` — applies to ALL adb calls uniformly. Both `adb()` and `adb_raw()` wrapped.

## Consequences

- No ADB call can exceed 30s regardless of device state
- A 20-iteration scroll_to with all misses: 20 × 30s = 10 min worst case (vs infinite before)
- Combined with scroll_deadline (60s) and TC deadline (300s): effective cap is whichever fires first
- Phase 4 unifies all timeout mechanisms into a single TimeoutManager
