# regression-android

Android Layer 2/3 local regression harness — demo app + runner. T6 mirror of
[regression-ios](../regression-ios) per cross-platform parity (doctrine #11).

## Phase status

- **Phase 1 — skeleton + script + scaffold**: SHIPPED (this commit-equivalent).
  Empty dirs, port of regress-ios.sh adapted for ddb/adb/AVD, fixtures.yaml +
  manifest.yaml ported with platform fields swapped. NO demo app, NO TC yamls.
- **Phase 2 — demo app + first ~5 TCs + agent loginHandler**: deferred to
  operator-wake review. Includes:
  - Kotlin/Compose demo app mirroring T1-T7 (HomeScreen + 7 screens) under
    `demo-app/app/src/main/kotlin/io/substrate/regdemo/`
  - `tests/t1.yaml..t7.yaml` authored against the demo screens
  - semantic-agent-android `loginHandler` companion-var add (Kotlin equivalent
    of iOS `SemanticAgent.loginHandler`) for T34 logged_in precondition
- **Phase 3 — full T1-T37 port + variance gate green**: multi-session
  (mirrors the iOS Batches A→B→C→D arc).

## Layout (Phase 1)

```
regression-android/
├── captures/            # transient, gitignored (* + !.gitignore)
├── catalogue/android/   # TD-44 visual-QA baselines (per-platform per Q4)
│   └── .gitkeep
├── demo-app/app/        # Kotlin/Compose scaffold (Phase 2 fills in)
│   └── src/main/{kotlin/io/substrate/regdemo, res/values}
├── docs/
├── recipes/             # login.yaml / logout.yaml (Phase 2 authors)
├── tests/               # TC yamls (Phase 2 ports t1..t7, Phase 3 t8..t37)
├── scripts/
│   └── regress-android.sh   # T1 — runner
├── fixtures.yaml
├── manifest.yaml
└── README.md
```

## Runner

`scripts/regress-android.sh` mirrors `regress-ios.sh` flag-for-flag (R1 clone
option per T6 design — doctrine #11 mirror over premature polymorphism).
Adaptations:

| iOS                              | Android                                     |
|----------------------------------|---------------------------------------------|
| `idb test run`                   | `ddb test`                                  |
| `xcrun simctl boot/install/launch` | `ddb adb install`, `am start`             |
| `--device sim-iphone16pro` (idb registry name) | `--device emu` (ddb registry name) |
| `BUNDLE_ID=io.substrate.regression-demo` | `PACKAGE=io.substrate.regdemo`        |
| `AGENT_PORT=9877`                | `AGENT_PORT=9876` (semantic-agent-android default) |
| `IDB_*` env vars                 | `DDB_*` env vars (incl. `DDB_TEST_PACKAGE`, `DDB_MAIN_ACTIVITY`, `DDB_EXPECTED_HASH`) |
| `--expect-fail` / `--variance N` / `--tests-dir DIR` / `--env-file FILE` / `--skip-install` / `--skip-layer2` / `--visual-qa` / `--update-baselines` | IDENTICAL flags + semantics (TD-44 visual-QA reused as-is — vdb diff is platform-agnostic) |

### Prerequisites

- AVD enrolled in ddb registry: `ddb devices add emu --emulator Pixel_8_API_34`
- AVD booted: `emulator -avd Pixel_8_API_34` (Phase 1 doesn't auto-boot; Phase 2 may)
- semantic-agent-android AAR consumed by demo-app (Phase 2 wiring)
- Build APK: `./gradlew assembleDebug` from `demo-app/` (Phase 2)

### Usage (post-Phase 2)

```sh
# Build APK first (Phase 2 once the gradle scaffold exists)
cd demo-app && ./gradlew assembleDebug

# Layer 3 only against pre-built APK
./scripts/regress-android.sh --skip-install --skip-layer2 --device emu --layer 3

# Layer 3 + visual-QA, variance ×3
./scripts/regress-android.sh --device emu --layer 3 --variance 3 --visual-qa
```

## Cross-platform parity policy

| Concern                 | Decision (per T6 design Q4)          |
|-------------------------|--------------------------------------|
| TC yaml source-of-truth | Per-platform `tests/` (TC1 — divergences surface naturally; consolidate post-hoc if needed). Cross-platform target.platform: forks via existing parser field. |
| Visual-QA baselines     | Per-platform `catalogue/android/<tc>.yaml` (Q4). Cross-platform vdb diff android.yaml vs ios.yaml emerges as a SECOND check phase if operator wants visual parity-gate. |
| Recipes                 | Per-platform `recipes/` (different verb names: ddb `tap_text` vs idb `tap_button` etc.). Schema shape identical. |
| Runner script           | R1 clone (no polymorphism). Two scripts, bounded duplication (~350 LOC each). Revisit (R2 polymorphic) only when bi-port pain warrants. |

## See also

- `../regression-ios/` — iOS-side mirror; T6 design doc lives in
  switchboard log + operator-wake review
- `../tctl/docs/tech-debt.md` — TD-44 visual-QA infra (Phase 1 shipped iOS,
  reused here verbatim)
