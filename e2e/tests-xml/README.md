# tests-xml/ — Epic M / M-2

XML-view variants of the Compose TC corpus at `../tests/`. One-for-one port of all 27 Compose TCs (`t1..t9`, `t10..t26`, `t34`).

## What this is

Same TC content as the Compose corpus, marked with:

- A header comment block citing Epic M / M-2 + the mirror file path.
- A top-level `app: io.substrate.regdemo.xml` field naming the target package (M-6 runner extension will honour it; current `ddb test` runner ignores unknown top-level keys harmlessly).

No step / target / assertion was modified. Each TC targets visible text via `content_fuzzy`; the XML demo (M-1, `../demo-app-xml/`) is expected to render the same labels for the test menu + per-screen widgets.

## What the runner must do (M-6)

1. Install the XML demo APK (`ddb/e2e/demo-app-xml/app/build/outputs/apk/debug/app-debug.apk`) instead of the Compose demo.
2. Target package `io.substrate.regdemo.xml`.
3. Otherwise: same regress-android.sh sweep flow.

Until M-6 ships, the operator can sweep manually:

```bash
nosandbox ./gradlew :demo-app-xml:assembleDebug
ddb -d <device> app install /tmp/demo-app-xml-debug.apk
ddb test run --suite tests-xml --tc <id>
```

## Coordination with M-1 (builder scaffold)

The XML demo must expose the same set of visible text labels the TCs target. The full label inventory below was extracted via `grep -hoE 'content_fuzzy: "[^"]+"' tests-xml/t*.yaml | sort -u`. If a label is missing from the XML demo, the TC that targets it will fail with `element not found`.

### Label inventory (84 unique strings)

Test-menu home screen entries (one per TC):

- T1 Launch, T2 Type, T3 Tap, T4 Navigate, T5 Keyboard, T6 Secure Field, T7 Dialog, T8 Scroll, T9 Wait, T10 Fetch, T11 Deep Nav, T12 Paste, T13 Login, T14 Anchor, T15 State, T16 Text Equal, T17 Spinner, T18 Tabs, T19 Sheet, T20 Counter, T21 Press Back, T22 Long Press, T23 Press, T24 Scroll Search, T25 Lazy List, T26 Toggle Visibility, T34 Cold Init

(Exact strings; some TCs target shortened forms, e.g. "T11 Deep" matches "T11 Deep Nav" via fuzzy.)

Per-screen widgets (selected — see `/tmp/xml-needed-labels.txt` for the full list):

- Generic: "OK", "Go Back", "Go Detail", "Detail Visible", "Show Alert", "Action Button", "Open T14 Anchor", "Dismiss Keyboard"
- T8 / T24: "scroll.top", "scroll.bottom", "Item 1" .. "Item 30"
- T10: "T10 Mock URL", "T10 Mocked Response Body"
- T11: "T11 Level 1", "T11 Level 2", "T11 Level 3"
- T12: "T12 Input", "T12 Paste", "T12 pasted content"
- T13: "T13 Login", "T13 Locked", "T13 Unlocked", "Invalid credentials"
- T14: "T14 Anchor", "T14 ScreenAnchor"
- T18: "T18 Tab Alpha Content", "T18 Tab Beta Content", "T18 Tab Gamma Content"
- T19: "T19 Show Sheet", "T19 Dismiss Sheet", "T19 Sheet Content"
- T20: "T20 Counter", "Increment", "3" (counter value assertion)
- T25: "T25 Item 30"
- T26: "T26 Target", "T26 Toggle Visibility", "shown", "hidden"

Element resource IDs (per M-1 dispatch convention `T<N>_<element-name>`) are NOT directly referenced by the TC corpus — only `content_fuzzy` text matching is used. The IDs are still required for the walker/visual-baseline pipeline (M-5).

## TC-level audit notes

- **TCs that exercise the agent's `/login` endpoint**: t13, t34. Recipe-loaded login flow is package-agnostic; should work on XML demo identically.
- **TCs that use `api_call`** to localhost:19878: t13 (login), t10 (mock URL), t34 (login + cold-init). Same agent port + endpoint convention as Compose demo.
- **TCs that use `press_back`**: t11, t21. XML demo's Activity stack should pop on back; verify in M-1 smoke.
- **No TC references `io.substrate.regdemo`** directly — package switch is purely install-side. Confirmed via `grep -lEr 'io.substrate.regdemo' tests/ recipes/` returning nothing.

## When to run

After builder ships M-1 (XML demo APK builds + installs + the test-menu home screen + all 27 screens render with the expected labels).

Per Wave-15-Phase-2 dispatch: **do not run yet**.
