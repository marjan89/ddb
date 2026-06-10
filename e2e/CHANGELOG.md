# Changelog

All notable changes to regression-android are documented here.

## [v0.3.0] — 2026-06-09

### Fixed
- **TD-101 Bug 1 / t34 logged_in precondition** (6744f95): `demo-app/app/src/main/kotlin/io/substrate/regdemo/RegdemoApplication.kt` — marshal `SemanticAgent.loginHandler` closure to the main looper via `Handler(Looper.getMainLooper()).post { T13Store.handle(...) }`. The handler was invoked from NanoHTTPD's HTTP worker thread; `T13Store.handle()` does `state.value = T13State.Unlocked` which is a Compose `mutableStateOf` write that requires main-thread mutation to reliably trigger recomposition. Off-main mutations sometimes worked (Compose is lenient) and sometimes didn't (race with frame schedule) — exact intermittent t34 signature.
- **TD-93 default RESET_MODE flip** (78b6612): `scripts/regress-android.sh` now defaults `RESET_MODE=am-restart`. The 5-sub-agent TD-93 drill converged on process survival across TCs as the dominant root cause of the deterministic 5-TC fail set in variance r=2/r=3 (t10/t12/t15/t25/t34): `SemanticServer`, `MockRegistry.shared`, and `T13Store` singletons inherit polluted state when am-restart is not forced between TCs. `RESET_MODE=none` remains available on the CLI for callers that explicitly want the prior behavior.

### Build
- **TD-99 init-script location** (04f8fd8): `demo-app/init.gradle.kts` — move the APK-copy init script out of `.gradle/init.d/` (a Gradle cache dir that is never loaded as init scripts) to a checked-in file. Build invocation needs `--init-script init.gradle.kts` to pick it up; the prior placement under `.gradle/init.d/` silently no-op'd.

### Verification
Variance×3 sweep after TD-101 Bug 1 (this repo) + Bug 2 (semantic-agent-android bf88b13) landed: **81/81 deterministic** across r=1, r=2, r=3 (per `/tmp/td101-verify-sweep.log` `all_pass: true`). Closes the Epic C v2 baseline restoration arc; t10/t12/t15/t25/t34 all stable.

## [v0.2.0] — 2026-06-09

### Fixed
- **TD-96** (d72cb6f): `tests/t12.yaml` step 5 `wait_idle` bumped 1s → 3s between the focus tap and the `POST /text-field/set` call. The 1s wait was too short for Compose's a11y tree to stabilize after focus, causing `findFocusedEditableVirtualId()` to walk a still-rebuilding tree and return null. Agent then returned 400 "no focused EditText". Surgical workaround; long-term fix is a `/text-field/focus-probe` agent endpoint (deferred).

### Notes
- Companion to TD-58 (Compose-aware focused-EditText fallback in semantic-agent-android) — TD-58 fix shape is correct; this is the timing follow-up.
- Part of the substrate-41c9 7-TC flake drill's ORTHOGONAL bucket; classification at `/tmp/td-flake-t12.md`.
