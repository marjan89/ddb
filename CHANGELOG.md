# Changelog

All notable changes to regression-android are documented here.

## [v0.2.0] — 2026-06-09

### Fixed
- **TD-96** (d72cb6f): `tests/t12.yaml` step 5 `wait_idle` bumped 1s → 3s between the focus tap and the `POST /text-field/set` call. The 1s wait was too short for Compose's a11y tree to stabilize after focus, causing `findFocusedEditableVirtualId()` to walk a still-rebuilding tree and return null. Agent then returned 400 "no focused EditText". Surgical workaround; long-term fix is a `/text-field/focus-probe` agent endpoint (deferred).

### Notes
- Companion to TD-58 (Compose-aware focused-EditText fallback in semantic-agent-android) — TD-58 fix shape is correct; this is the timing follow-up.
- Part of the substrate-41c9 7-TC flake drill's ORTHOGONAL bucket; classification at `/tmp/td-flake-t12.md`.
