# Changelog

All notable changes to ddb are documented here.

## [v0.4.0] — 2026-06-10 — Epic G ddb-absorption monorepo

### Added
- **Epic G ddb-absorption** (Pass-0 `ea3af3a` semantic-schema submodule + `d593913` `git subtree add agent/` + `7558676` `git subtree add e2e/`): ddb is now a STANDALONE MONOREPO containing — Rust CLI (`src/`), `agent/` (Kotlin SemanticAgent, absorbed from `semantic-agent-android@a5d9bafd`), `e2e/` (Gradle demo + YAML TCs, absorbed from `regression-android@cb5c6f6`), `semantic-schema/` (git submodule). Per ADR-011 absorption mechanics.
- **Pass-3 path rewires** (`1790738` + `bca5ed7`): `e2e/demo-app` `includeBuild` repointed to the absorbed `agent/`, agent `gradle.properties` restored with `android.useAndroidX=true`.
- **Defensive .gitignore** (`2a6aab6` + `b419fdd`): post-absorption stale Gradle artifacts untracked; `*.hprof` / `*.dump` / `*.heapdump` ignored across the monorepo.
- **CI workflow** (`618e4d3`): GitHub Actions for the monorepo — `rust` job (build + clippy), `android` job (Gradle build for `agent` + `e2e`), `release` job (tag-triggered binary publish for `ddb-v*`). Mirrors idb's CI workflow shape.

### Notes
- Pre-absorption wrapper retired; tag style is monolithic (`ddb-vX.Y.Z`). Head pushed to `github.com/marjan89/ddb` as tag `ddb-v0.4.0`.
- See `substrate-distro/tctl/docs/epics.md` §Epic G + `tctl/docs/adr/ADR-011-cross-repo-absorption-mechanics.md` for the full absorption pattern.

## [v0.3.1] — 2026-06-09

### Fixed
- **TD-94** (776cc70): warm `/health` timeout bumped 5s → 15s in `cmd/test.rs` to accommodate AAR republish + agent cold-restart settle time. Cold-start default (120s) untouched; `DDB_AGENT_READY_TIMEOUT` env override preserved. Eliminates t8 flake from sweeps that exercise the `agent_login` path without `--skip-install`.

## [v0.3.0] — 2026-06-08

### Added
- **Epic K observability hardening** (ce353fc): 8 surfaces behind `--observability` flag — ERROR banner with ANSI-red + bell, fail-fast on infra errors (FD leak, agent unreachable, sim disconnect), real-time WARN streaming, `[PROGRESS N/T fails=X FD=Y]` heartbeat every 5 TCs, FD watchdog via `/proc/self/fd`, agent health pings between TCs, per-step structured logs, `--log-format=json` for machine-parseable output. Mirrors idb's Epic K surfaces. See ADR-009.

### Notes
- ddb tracks idb's observability surfaces 1:1 — same dashboards / parsers / alerting work for both runners.
- Epic K stabilization is in-flight; surfaces ride behind `--observability` until baseline noise is characterized.
