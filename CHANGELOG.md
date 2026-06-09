# Changelog

All notable changes to ddb are documented here.

## [v0.3.1] — 2026-06-09

### Fixed
- **TD-94** (776cc70): warm `/health` timeout bumped 5s → 15s in `cmd/test.rs` to accommodate AAR republish + agent cold-restart settle time. Cold-start default (120s) untouched; `DDB_AGENT_READY_TIMEOUT` env override preserved. Eliminates t8 flake from sweeps that exercise the `agent_login` path without `--skip-install`.

## [v0.3.0] — 2026-06-08

### Added
- **Epic K observability hardening** (ce353fc): 8 surfaces behind `--observability` flag — ERROR banner with ANSI-red + bell, fail-fast on infra errors (FD leak, agent unreachable, sim disconnect), real-time WARN streaming, `[PROGRESS N/T fails=X FD=Y]` heartbeat every 5 TCs, FD watchdog via `/proc/self/fd`, agent health pings between TCs, per-step structured logs, `--log-format=json` for machine-parseable output. Mirrors idb's Epic K surfaces. See ADR-009.

### Notes
- ddb tracks idb's observability surfaces 1:1 — same dashboards / parsers / alerting work for both runners.
- Epic K stabilization is in-flight; surfaces ride behind `--observability` until baseline noise is characterized.
