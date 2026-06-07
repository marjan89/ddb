#!/usr/bin/env bash
# regress-android.sh — local Android Layer 2/3 regression runner.
# T6 mirror of regress-ios.sh (doctrine #11 cross-platform parity).
#
# Layer 2: agent smoke (/health 200 within 30s, /version SHA captured).
# Layer 3: golden T1-T37 YAML TCs run via `ddb test`.
# Variance gate: --variance N runs Layer 3 back-to-back N times.
#
# Exit codes:
#   0 — all gates pass
#   1 — gate failure (Layer 2 or any Layer 3 TC)
#   2 — preflight failure (missing tool, missing app, emu unavailable)
#
# T6 PHASE STATUS:
# - Phase 1 (skeleton + script + scaffold): SHIPPED tonight, no demo app yet
# - Phase 2 (demo app + first ~5 TCs + agent loginHandler): operator-wake
# - Phase 3 (full T1-T37 port + variance gate green): multi-session

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REGRESSION_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="$REGRESSION_ROOT/tests"

LAYER="all"
VARIANCE=1
DEVICE_NAME="emu"
APP_PATH="${DEMO_APP_PATH:-$REGRESSION_ROOT/demo-app/app/build/outputs/apk/debug/app-debug.apk}"
PACKAGE="io.substrate.regdemo"
MAIN_ACTIVITY="io.substrate.regdemo/.MainActivity"
AVD_NAME="Pixel_8_API_34"
AGENT_PORT=9876
SKIP_INSTALL=0
SKIP_LAYER2=0
ENV_FILE=""
VERSION_SHA=""
# TD-44: visual-QA phase 4 controls (default off; opt-in initially).
VISUAL_QA=0
UPDATE_BASELINES=0
# TD-50: per-TC reset mode + prewarm (Android sibling of iOS TD-49).
# Default mode=none for back-compat; am-restart force-stops + relaunches
# the demo before each TC so state from prior TC's navigation doesn't
# carry over (T2 leaves T2TypeScreen → T1's "T1 Launch" assert fails).
RESET_MODE="none"
PREWARM=0
# TD-57: peer apps on the device that also embed the semantic-agent and
# bind canonical port 9876. Whichever process binds first serves /semantic;
# losers' a11y trees become unreachable. Force-stop these BEFORE the sweep
# so regdemo wins the bind every time.
PEER_AGENT_PACKAGES=(
  "se.naturkartan.android"
)
EXPECT_FAIL_LIST=()
# Two parallel arrays keyed by index: TC name → required substring in failure log.
EXPECT_FAIL_MSG_TC=()
EXPECT_FAIL_MSG_SUB=()

usage() {
  cat <<EOF
Usage: $0 [options]
  --layer {2|3|all}   default: all
  --variance N        default: 1   (runs Layer 3 back-to-back N times)
  --device NAME       default: emu (ddb device registry name)
  --app-path PATH     default: \$DEMO_APP_PATH or demo-app/app/build/outputs/apk/debug/app-debug.apk
  --skip-install      reuse already-installed app (dev loop ergonomics)
  --tests-dir DIR     override TESTS_DIR (default: <repo>/tests). Use to point
                      at a production catalogue dir without copying files.
  --env-file FILE     source FILE (set -a / source / set +a) so KEY=value
                      lines export as env vars (DDB_TEST_EMAIL, DDB_TEST_PASSWORD,
                      DDB_FIXTURES_PATH, DDB_HOME_TAB, DDB_RECIPE_DIR, etc.)
                      visible to idb test run + login.yaml env interpolation.
  --skip-layer2       skip /health smoke probe (production builds don't boot
                      the semantic agent unless #if DEBUG). Mutually exclusive
                      with --layer 2.
  --visual-qa         enable TD-44 post-sweep visual-QA: per-TC vdb diff of
                      captured semantic YAML vs baseline at catalogue/android/<tc>.yaml.
                      Errors (MISSING / WRONG_TEXT) fail the gate; warnings
                      flag but pass. Baselines missing → TC skipped. Opt-in.
  --update-baselines  copy current sweep captures (captures/<tc>-semantic.yaml)
                      over catalogue/android/<tc>.yaml baselines. Manual operator
                      trigger after visual verification. Implies --visual-qa.
  --expect-fail TC    invert exit-code semantics for one TC (repeatable).
                      Accepts a basename ('t16-negative') or absolute path.
                      Runner FAILS when the TC asserts PASS; PASSES when assert fails.
                      Use for self-tests that prove a verb's enforcement (e.g. 97368e9).
  --reset-mode {none|am-restart}
                      default: none. am-restart force-stops + relaunches
                      the demo via 'adb shell am force-stop <pkg>' +
                      'adb shell am start -n <pkg>/<activity>' before EACH
                      Layer-3 TC, so prior TC's navigation state doesn't
                      bleed in (TD-50 Android mirror of iOS TD-49).
  --prewarm           pre-TC0 launch + /health poll until 200 (15s budget).
                      Eliminates first-TC race where am start lags the
                      runner's first /idle call. Logs [TD-50][prewarm].
  --expect-fail-message TC=SUBSTR
                      Pair with --expect-fail to also require SUBSTR in the failure log.
                      Without it, gate FAILS with reason 'expected-fail-message-mismatch'.
                      Used to differentiate idb binaries whose assertions both fail but
                      for different reasons (e.g. parent: 'element not found' vs
                      patched: 'text mismatch'). Repeatable.
  -h, --help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --layer)        LAYER="$2"; shift 2;;
    --variance)     VARIANCE="$2"; shift 2;;
    --device)       DEVICE_NAME="$2"; shift 2;;
    --app-path)     APP_PATH="$2"; shift 2;;
    --skip-install) SKIP_INSTALL=1; shift;;
    --tests-dir)    TESTS_DIR="$2"; shift 2;;
    --env-file)     ENV_FILE="$2"; shift 2;;
    --skip-layer2)  SKIP_LAYER2=1; shift;;
    --visual-qa)    VISUAL_QA=1; shift;;
    --update-baselines) UPDATE_BASELINES=1; VISUAL_QA=1; shift;;
    --reset-mode)
      RESET_MODE="$2"
      case "$RESET_MODE" in
        none|am-restart) ;;
        *) echo "preflight: --reset-mode must be {none|am-restart} (got: $RESET_MODE)" >&2; exit 2;;
      esac
      shift 2;;
    --prewarm)      PREWARM=1; shift;;
    --expect-fail)  EXPECT_FAIL_LIST+=("$2"); shift 2;;
    --expect-fail-message)
      # Form: TC=SUBSTR (split on first '=')
      ef_arg="$2"
      ef_tc="${ef_arg%%=*}"
      ef_sub="${ef_arg#*=}"
      if [[ "$ef_tc" == "$ef_arg" ]] || [[ -z "$ef_sub" ]]; then
        echo "preflight: --expect-fail-message expects TC=SUBSTR (got: $ef_arg)" >&2
        exit 2
      fi
      EXPECT_FAIL_MSG_TC+=("$ef_tc")
      EXPECT_FAIL_MSG_SUB+=("$ef_sub")
      shift 2
      ;;
    -h|--help)      usage; exit 0;;
    *)              echo "preflight: unknown flag: $1" >&2; exit 2;;
  esac
done

# ─── PHASE 0: preflight ───────────────────────────────────────────────
preflight() {
  local missing=()
  for tool in adb emulator ddb jq curl; do
    command -v "$tool" >/dev/null || missing+=("$tool")
  done
  if (( ${#missing[@]} > 0 )); then
    echo "preflight: missing tool(s): ${missing[*]}" >&2
    exit 2
  fi

  if (( SKIP_LAYER2 == 1 )) && [[ "$LAYER" == "2" ]]; then
    echo "preflight: --skip-layer2 + --layer 2 are mutually exclusive" >&2
    exit 2
  fi

  if [[ ! -d "$TESTS_DIR" ]]; then
    echo "preflight: --tests-dir not found: $TESTS_DIR" >&2
    exit 2
  fi
  local yaml_count
  yaml_count=$(find "$TESTS_DIR" -maxdepth 1 -name '*.yaml' 2>/dev/null | wc -l | tr -d ' ')
  echo "[T3][tests-dir] using $TESTS_DIR ($yaml_count yamls found)" >&2

  # NOTE: env-file sourcing intentionally NOT done here — preflight() runs
  # in a command-substitution subshell (UDID=$(preflight)), so vars sourced
  # in this scope would not propagate to the parent shell. The env-file is
  # validated here and sourced in main below, where it stays visible to
  # the layer3_golden loop.
  if [[ -n "$ENV_FILE" ]] && [[ ! -r "$ENV_FILE" ]]; then
    echo "preflight: --env-file not readable: $ENV_FILE" >&2
    exit 2
  fi

  if (( SKIP_LAYER2 == 1 )); then
    echo "[T3][skip-layer2] layer2 smoke disabled (production-mode)" >&2
  fi

  if [[ "$LAYER" != "2" ]] && (( SKIP_INSTALL == 0 )); then
    if [[ ! -f "$APP_PATH" ]]; then
      echo "preflight: APK not found at $APP_PATH" >&2
      echo "          set DEMO_APP_PATH or --app-path, or pass --skip-install" >&2
      exit 2
    fi
  fi

  # Identify a booted emulator that matches the ddb-registered DEVICE_NAME.
  # Unlike iOS where we identify sim by name+udid, Android side: ddb has a
  # device registry (~/.config/ddb/devices.toml) keyed by friendly name; we
  # trust the operator's registry and just verify the device is reachable.
  if ! ddb -d "$DEVICE_NAME" adb devices 2>/dev/null | grep -q "device$"; then
    echo "preflight: ddb device '$DEVICE_NAME' not reachable" >&2
    echo "          enroll via: ddb devices add $DEVICE_NAME --emulator $AVD_NAME" >&2
    exit 2
  fi
  echo "$DEVICE_NAME"
}

# ─── PHASE 1: boot + enroll + install ─────────────────────────────────
# Android variant: AVD is operator-booted out-of-band OR via 'emulator -avd'.
# Phase 1 here just (re-)installs the APK and launches the main activity.
# TODO(C2 — operator-wake): wire AVD auto-boot if not running.
boot_and_install() {
  local dev_name="$1"

  if (( SKIP_INSTALL == 0 )); then
    echo "phase1: installing $APP_PATH on $dev_name..." >&2
    ddb -d "$dev_name" adb install -r "$APP_PATH" \
      || { echo "phase1: install failed" >&2; exit 1; }
  fi

  # TD-57: stop peer agent-bearing apps so regdemo wins port 9876.
  local peer
  for peer in "${PEER_AGENT_PACKAGES[@]}"; do
    if ddb -d "$dev_name" adb shell pidof "$peer" 2>/dev/null | grep -q '[0-9]'; then
      ddb -d "$dev_name" adb shell am force-stop "$peer" >/dev/null 2>&1 || true
      echo "[TD-57][peer-stop] force-stop $peer (claimed agent port 9876)" >&2
    fi
  done

  echo "phase1: launching $MAIN_ACTIVITY on $dev_name..." >&2
  ddb -d "$dev_name" adb shell am start -n "$MAIN_ACTIVITY" \
    || { echo "phase1: launch failed" >&2; exit 1; }
}

# TD-50: force-stop + relaunch the demo. Used for --prewarm and per-TC
# --reset-mode=am-restart. Silent on success; logs with [TD-50] marker.
reset_demo() {
  local dev_name="$1"
  local context="${2:-reset}"
  ddb -d "$dev_name" adb shell am force-stop "$PACKAGE" >/dev/null 2>&1 || true
  ddb -d "$dev_name" adb shell am start -n "$MAIN_ACTIVITY" >/dev/null 2>&1 \
    || { echo "[TD-50][$context] am start failed for $MAIN_ACTIVITY" >&2; return 1; }
  echo "[TD-50][$context] force-stop + am start $PACKAGE" >&2
  return 0
}

# TD-50: pre-TC0 prewarm. force-stop+relaunch then poll /health on the
# device-side agent port (forwarded locally) until 200 or 15s deadline.
prewarm_demo() {
  local dev_name="$1"
  reset_demo "$dev_name" "prewarm" || return 1
  ddb -d "$dev_name" adb forward "tcp:$AGENT_PORT" "tcp:$AGENT_PORT" >/dev/null 2>&1 || true
  local deadline=$(( SECONDS + 15 ))
  while (( SECONDS < deadline )); do
    if curl -sf --max-time 2 "http://127.0.0.1:$AGENT_PORT/health" -o /dev/null; then
      echo "[TD-50][prewarm] /health 200 (port $AGENT_PORT)" >&2
      return 0
    fi
    sleep 1
  done
  echo "[TD-50][prewarm] /health did not return 200 within 15s" >&2
  return 1
}

# ─── PHASE 2: Layer 2 smoke ───────────────────────────────────────────
layer2_smoke() {
  local agent_url="http://localhost:$AGENT_PORT"
  local deadline=$(( SECONDS + 30 ))
  echo "layer2: polling $agent_url/health (30s budget)..." >&2
  while (( SECONDS < deadline )); do
    if curl -sf --max-time 3 "$agent_url/health" -o /dev/null; then
      echo "layer2: /health OK" >&2
      local version_json
      version_json=$(curl -sf --max-time 3 "$agent_url/version" || true)
      VERSION_SHA=$(echo "$version_json" | jq -r '.git_hash // empty' 2>/dev/null || true)
      if [[ -z "$VERSION_SHA" ]]; then
        echo "layer2: /version did not return git_hash (got: $version_json)" >&2
        return 1
      fi
      echo "layer2: /version git_hash=$VERSION_SHA" >&2
      return 0
    fi
    sleep 2
  done
  echo "layer2: /health did not return 200 within 30s" >&2
  return 1
}

# Returns 0 if $1 is in the --expect-fail list (matches basename OR absolute path).
is_expect_fail() {
  local tc_name="$1"
  local yaml_path="$2"
  local ef
  for ef in "${EXPECT_FAIL_LIST[@]}"; do
    if [[ "$ef" == "$tc_name" ]] || [[ "$ef" == "$yaml_path" ]] || [[ "$ef" == "$tc_name.yaml" ]]; then
      return 0
    fi
  done
  return 1
}

# Echoes the required failure-log substring for a TC if --expect-fail-message
# was supplied for it; empty otherwise. Pre-bash-4 safe (no associative arrays).
expect_fail_substr_for() {
  local tc_name="$1"
  local i=0
  while (( i < ${#EXPECT_FAIL_MSG_TC[@]} )); do
    if [[ "${EXPECT_FAIL_MSG_TC[$i]}" == "$tc_name" ]]; then
      echo "${EXPECT_FAIL_MSG_SUB[$i]}"
      return 0
    fi
    i=$((i + 1))
  done
  return 1
}

# ─── PHASE 3: Layer 3 golden ──────────────────────────────────────────
# Iterates every tests/*.yaml in alphabetical order. --expect-fail flips the
# exit-code interpretation for the listed TCs: a passing assertion becomes a
# gate failure, a failing assertion becomes the expected outcome.
layer3_golden() {
  local run_n="$1"
  local -a results=()
  local yaml
  for yaml in "$TESTS_DIR"/*.yaml; do
    [[ -f "$yaml" ]] || continue
    local tc
    tc=$(basename "$yaml" .yaml)
    local log="/tmp/regress-android-$tc-r$run_n.log"
    local expect_fail=0
    if is_expect_fail "$tc" "$yaml"; then expect_fail=1; fi

    echo "layer3 r=$run_n: $tc ...$( ((expect_fail)) && echo ' [expect-fail]')" >&2
    if [[ "$RESET_MODE" == "am-restart" ]]; then
      reset_demo "$DEVICE_NAME" "reset-mode=am-restart" || true
      sleep 1
    fi
    local actual
    # ddb test does not currently support --catalogue arg the same way idb
    # does; T6 Phase 2 may need a ddb-side analog. For now, fixtures.yaml
    # interpolation lives in ddb via DDB_FIXTURES_PATH env var.
    if DDB_TEST_PACKAGE="$PACKAGE" DDB_MAIN_ACTIVITY="$MAIN_ACTIVITY" DDB_EXPECTED_HASH="${VERSION_SHA:-ignored}" DDB_FIXTURES_PATH="${DDB_FIXTURES_PATH:-$REGRESSION_ROOT/fixtures.yaml}" DDB_RECIPE_DIR="${DDB_RECIPE_DIR:-$REGRESSION_ROOT/recipes}" DDB_LOGIN_RECIPE="${DDB_LOGIN_RECIPE:-$REGRESSION_ROOT/recipes/login.yaml}" DDB_LOGOUT_RECIPE="${DDB_LOGOUT_RECIPE:-$REGRESSION_ROOT/recipes/logout.yaml}" DDB_LOGGED_IN_INDICATOR="${DDB_LOGGED_IN_INDICATOR:-T13 Unlocked}" ddb test -d "$DEVICE_NAME" "$yaml" >"$log" 2>&1; then
      actual=pass
    else
      actual=fail
    fi

    local status
    if (( expect_fail )); then
      if [[ "$actual" == "fail" ]]; then
        local required_sub
        if required_sub=$(expect_fail_substr_for "$tc"); then
          if grep -qF "$required_sub" "$log"; then
            status=expected-fail-pass
            echo "layer3 r=$run_n: $tc PASS (asserted failure with required substring '$required_sub')" >&2
          else
            status=expected-fail-message-mismatch
            echo "layer3 r=$run_n: $tc FAIL (asserted failure but log missing required substring '$required_sub'; log: $log)" >&2
          fi
        else
          status=expected-fail-pass
          echo "layer3 r=$run_n: $tc PASS (asserted failure as expected)" >&2
        fi
      else
        status=passed-but-expected-fail
        echo "layer3 r=$run_n: $tc FAIL (expected failure but assert passed; log: $log)" >&2
      fi
    else
      status=$actual
      if [[ "$actual" == "pass" ]]; then
        echo "layer3 r=$run_n: $tc PASS" >&2
      else
        echo "layer3 r=$run_n: $tc FAIL (log: $log)" >&2
      fi
    fi

    results+=("{\"tc\":\"$tc\",\"status\":\"$status\",\"variance_run\":$run_n,\"expect_fail\":$( ((expect_fail)) && echo true || echo false )}")
  done
  ( IFS=,; echo "${results[*]}" )
}

# ─── PHASE 4: summary ─────────────────────────────────────────────────
emit_summary() {
  local l2_status="$1"
  local l3_json="$2"
  local all_pass="$3"
  local vqa_json="${4:-null}"
  jq -n \
    --arg   l2    "$l2_status" \
    --argjson l3  "[${l3_json:-}]" \
    --argjson v   "$VARIANCE" \
    --arg   sha   "${VERSION_SHA:-}" \
    --argjson all "$all_pass" \
    --argjson vqa "$vqa_json" \
    '{layer2:$l2, layer3:$l3, variance_runs:$v, agent_sha:$sha, all_pass:$all, visual_qa:$vqa}'
}

# ─── PHASE 4: visual-QA (TD-44) ───────────────────────────────────────
# Post-sweep per-TC vdb diff: catalogue/android/<tc>.yaml (baseline) vs
# captures/<tc>-semantic.yaml (current sweep). ERRORS fail the gate;
# WARNINGS flag but pass. Missing baseline → skip (operator hasn't
# seeded yet). Missing capture → skip (TC didn't run capture verb).
# Opt-in via --visual-qa. --update-baselines copies captures over
# baselines (operator manual trigger).
visual_qa() {
  local catalogue_dir="$REGRESSION_ROOT/catalogue/android"
  local captures_dir="$REGRESSION_ROOT/captures"
  local per_tc_json=""
  local mismatch_count=0
  local missing_baseline=0
  local missing_capture=0
  local pass_count=0
  local pass=true

  for tc_yaml in "$TESTS_DIR"/*.yaml; do
    [[ -f "$tc_yaml" ]] || continue
    local tc; tc=$(basename "$tc_yaml" .yaml)
    local baseline="$catalogue_dir/$tc.yaml"
    local captured="$captures_dir/$tc-semantic.yaml"
    local entry

    if [[ ! -f "$captured" ]]; then
      echo "[T44][visual-qa] $tc SKIP (no capture at $captured)" >&2
      missing_capture=$((missing_capture + 1))
      entry="{\"tc\":\"$tc\",\"status\":\"no-capture\"}"
    elif (( UPDATE_BASELINES == 1 )); then
      mkdir -p "$catalogue_dir"
      cp "$captured" "$baseline"
      echo "[T44][visual-qa] $tc BASELINE-UPDATED ($baseline)" >&2
      entry="{\"tc\":\"$tc\",\"status\":\"baseline-updated\"}"
    elif [[ ! -f "$baseline" ]]; then
      echo "[T44][visual-qa] $tc SKIP (no baseline at $baseline)" >&2
      missing_baseline=$((missing_baseline + 1))
      entry="{\"tc\":\"$tc\",\"status\":\"no-baseline\"}"
    else
      local diff_json err_count
      diff_json=$(vdb diff --json "$baseline" "$captured" 2>/dev/null)
      err_count=$(echo "$diff_json" | jq '.errors | length')
      if (( err_count == 0 )); then
        echo "[T44][visual-qa] $tc PASS" >&2
        pass_count=$((pass_count + 1))
        entry="{\"tc\":\"$tc\",\"status\":\"pass\",\"errors\":0}"
      else
        echo "[T44][visual-qa] $tc DRIFT (errors: $err_count)" >&2
        mismatch_count=$((mismatch_count + 1))
        pass=false
        entry="{\"tc\":\"$tc\",\"status\":\"drift\",\"errors\":$err_count}"
      fi
    fi
    if [[ -n "$per_tc_json" ]]; then per_tc_json="$per_tc_json,$entry"; else per_tc_json="$entry"; fi
  done

  echo "[T44][visual-qa] sweep done: pass=$pass_count drift=$mismatch_count no-baseline=$missing_baseline no-capture=$missing_capture" >&2
  jq -n \
    --argjson pass "$( $pass && echo true || echo false )" \
    --argjson mismatches "$mismatch_count" \
    --argjson missing_baseline "$missing_baseline" \
    --argjson missing_capture "$missing_capture" \
    --argjson pass_count "$pass_count" \
    --argjson per_tc "[${per_tc_json:-}]" \
    '{pass:$pass, mismatches:$mismatches, missing_baseline:$missing_baseline, missing_capture:$missing_capture, pass_count:$pass_count, per_tc:$per_tc}'
  $pass
}

# ─── main ─────────────────────────────────────────────────────────────
UDID=$(preflight)
[[ -n "$UDID" ]] || exit 2

# Source env-file in MAIN shell so layer3_golden inherits exported vars.
# (See preflight() note above re: subshell isolation.)
if [[ -n "$ENV_FILE" ]]; then
  before=$(env | wc -l | tr -d ' ')
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
  after=$(env | wc -l | tr -d ' ')
  echo "[T3][env-file] sourced $ENV_FILE ($((after - before)) new vars exported)" >&2
fi

if [[ "$LAYER" != "2" ]]; then
  boot_and_install "$UDID"
fi

L2_STATUS="skipped"
L3_AGG=""
ALL_PASS=true

if (( SKIP_LAYER2 == 1 )); then
  L2_STATUS="skipped-by-flag"
elif [[ "$LAYER" == "2" || "$LAYER" == "all" ]]; then
  if layer2_smoke; then L2_STATUS="pass"; else L2_STATUS="fail"; ALL_PASS=false; fi
fi

if [[ "$LAYER" == "3" || "$LAYER" == "all" ]] && [[ "$L2_STATUS" != "fail" ]]; then
  for (( run=1; run<=VARIANCE; run++ )); do
    chunk=$(layer3_golden "$run")
    if [[ -n "$L3_AGG" ]]; then L3_AGG="$L3_AGG,$chunk"; else L3_AGG="$chunk"; fi
    if echo "$chunk" | grep -qE '"status":"(fail|missing|passed-but-expected-fail|expected-fail-message-mismatch)"'; then ALL_PASS=false; fi
  done
fi

# TD-44: Phase 4 visual-QA. Opt-in via --visual-qa. Captures must be
# produced by TC capture verbs into $REGRESSION_ROOT/captures/<tc>-semantic.yaml.
VQA_JSON="null"
if (( VISUAL_QA == 1 )); then
  if VQA_JSON=$(visual_qa); then :; else ALL_PASS=false; fi
fi

emit_summary "$L2_STATUS" "$L3_AGG" "$ALL_PASS" "$VQA_JSON"
$ALL_PASS && exit 0 || exit 1
