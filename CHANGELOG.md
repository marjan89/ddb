# Changelog

All notable changes to ddb are documented here.

## [Unreleased] — 2026-06-11 — Epic M ✓ CLOSED + Wave 15-19 ledger

### Wave 19 — TD-128 reset_demo forward race fix + M-10 canonical
- **TD-128 / reset_demo forward race** (`c1c16cd`): root-cause fix for the cross-app port-collision class that aborted XML sweeps pre-TC1. After `am force-stop $PACKAGE` + `am start $PACKAGE/$MAIN_ACTIVITY` inside `reset_demo()`, the per-device adb forward state can be invalidated; the next `/health` probe lands on a wedged forward. Fix: hoist `ensure_agent_forward` + 1s settle into `reset_demo` so each per-TC restart re-establishes the forward atomically. 2-line core change; discriminator instrumentation stripped before commit.
  - Discriminating test confirmation: post-`reset_demo` `/health` returns 200 on every TC across a full sweep when forward is re-established; baseline (no re-forward) returned 000 from TC 2 onward. Wave-19 research validated builder's hypothesis (forward-state corruption) over the empty-`git_hash`-kills-agent dispatch premise (which was wrong — Compose has same empty `git_hash` and stays alive).
- **M-10 canonical XML baselines** (`24f747d`): post-TD-128 sweep — `regress-android.sh --target xml --update-baselines --variance 1` now runs through. 21 of 31 `catalogue/android-xml/*.yaml` baselines refreshed with runner-produced captures (in-TC `action: capture` step). Sweep tally: **14 PASS / 10 FAIL** of the XML corpus; the 10 FAILs are TC-tuning gaps now visible because the runner actually reaches each TC's steps (Epic J parity-rule scoped).

### Wave 18 — install pattern (TD-125 close)
- **TD-125 install-pattern across ddb monorepo** (`2886566`): `README.md` + `docs/README.md` flipped `cp target/release/ddb ...` → `install -m 755 target/release/ddb ...` for atomic replace. Same pattern adopted across idb / fdb / vdb docs in their respective repos. Fixes the macOS-codesign-cache SIGKILL class that produced the post-rebuild "binary returns empty stdout" symptom.

### Wave 15 — Epic M (cross-framework parity) — ✓ CLOSED 2026-06-11



### Added
- **Epic M / M-1 — XML view demo app** (`9fa3fba` + `370454f` + `8e8f4e0` + `f460980`): new `e2e/demo-app-xml/` (sibling to `e2e/demo-app/`). Gradle scaffold mirrors the Compose demo; single `MainActivity` hosts 27 layouts (`res/layout/activity_t<N>.xml`) corresponding to T1..T26 + T34. `android.widget.*` only, no Compose. Bundle id `io.substrate.regdemo.xml`, Kotlin package + Gradle namespace aligned with applicationId (`.MainActivity` resolves canonically). Sibling APK output via `e2e/demo-app-xml/.gradle/init.d/copy-apk.gradle.kts`.
- **Epic M / M-2 — XML TC corpus** (`7e4a164` + `b44e17a`): `e2e/tests-xml/` mirrors `e2e/tests/` 1-to-1 (27 TCs: t1..t9, t10..t26, t34). Each TC carries a 4-line header citing Epic M / M-2 + the canonical app id (`app: io.substrate.regdemo.xml`). Audit confirmed all 27 TCs target visible text via `content_fuzzy`; no resource-id hardcoding. Smoke via `regress-android.sh --target xml`: 27/27 PASS first-try.
- **Epic M / M-3 — 7 iOS-only Compose ports** (`0dd09f0`): t16-negative, t27, t31, t32, t33, t35, t37 ported from `regression-ios/tests/` to `e2e/tests/`. Capture paths rewritten regression-ios → ddb tree. Audit: zero new Compose screens needed; all required elements + fixture interpolation block (`homescreen.first_link/spinner_link/spinner_target`) already present.
- **Epic M / M-4 — 7 iOS-only XML ports** (`1b8df16`): parity with M-3 — same 7 TCs ported to `e2e/tests-xml/`. Sub-agent verification confirmed Compose-side audit holds for XML: no new screens needed.
- **Epic M / M-5 — XML visual baselines** (`02c2698` partial + `ddbf449` clean): `e2e/catalogue/android-xml/` populated with per-TC navigated state captures (27/27, 18 unique sizes, 1622-15700 byte range). Initial Wave-15-Phase-3 seed produced uniform home-only captures; clean re-seed via `/tmp/m5-perTC-seed.py` replays each TC's pre-capture steps via direct adb+curl (bypasses runner) for proper per-TC differentiation. visual-QA dir-switch `dfebc2d` enables `--target xml` baseline path (`catalogue/android-xml/` vs `catalogue/android/`).
- **Epic M / M-6 — `regress-android.sh --target {compose|xml}`** (`d23e55e`): single flag switches `APP_PATH` + `TESTS_DIR` + `PACKAGE` + `MAIN_ACTIVITY` as a unit. Defaults `compose` for back-compat.

### Fixed
- **TD-124 PART-1 — agent /health preflight retry** (`f9f2bc3`): `src/cmd/test_observability.rs` agent_health_ping now retries 5x with 200/280/360/440 ms exponential backoff (~1.6s worst-case) before declaring `agent_unhealthy`. Mirrors the prior-art pattern at `test.rs:828-855`. Covers transient race between `am start` and first `/health` 200 response. (Part 2 — the deeper /version-kills-XML-agent behavior — deferred under TD-125.)
- **regress-android.sh `--target xml` path resolution**: builder-side namespace alignment (`f460980`) lets `<pkg>/.MainActivity` Intent resolve cleanly under the canonical `io.substrate.regdemo.xml/.MainActivity` form.

### Known issue
- **TD-125 — ddb binary "shadow ghost" after rebuild** (partial answer at tctl `c7028c5`): after a `cargo build --release`, the PATH-resolved `ddb` binary can return empty stdout from `--version` / subcommand `--help` calls until reinstalled to BOTH `/opt/cargo/bin/ddb` AND `/opt/homebrew/bin/ddb`. Workaround: `rm /opt/cargo/bin/ddb /opt/homebrew/bin/ddb && cp target/release/ddb /opt/cargo/bin/ddb && cp target/release/ddb /opt/homebrew/bin/ddb && hash -r`. Root-cause investigation deferred.

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
