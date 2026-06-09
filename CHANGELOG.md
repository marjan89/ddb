# Changelog

All notable changes to semantic-agent-android are documented here.

## [v0.3.0] — 2026-06-09

### Added
- **TD-91 + TD-92 + TD-59 incompleteness** (11c6642): Compose semantics walker now uses `awaitFrame()` vsync barriers in place of `Thread.sleep` + `mainHandler.post` loops, eliminating the Choreographer-starvation race that produced stale snapshots in t7/t15/t25.
  - `handleScrollSearch` (TD-91): dispatches `AccessibilityAction.ACTION_SCROLL_FORWARD` on the LazyColumn's a11y node, then 2× `awaitFrame()` — one for scroll application, one for LazyList item realization — before resample. Replaces the leaky MotionEvent swipe path (TD-63 predecessor, reverted).
  - `handleSemantic` (TD-92): replaces `mainHandler.post` + `Thread.sleep(100)` loop with `GlobalScope.launch(Dispatchers.Main) + awaitFrame()` suspension. Recompose can complete between checks; no observer-effect from raw `Choreographer.postFrameCallback`.
  - `ViewTreeWalker` (TD-59): `enabled` field derived from `!isDisabled` instead of hardcoded `true`. Closes the walker-completeness gap surfaced by substrate-41c9's 7-TC flake drill.
- **TD-95 (bundled)** (11c6642): `handleMock` now unwraps nested object bodies (`response.body.{...}`) via opt-then-toString JSON encoding instead of `optString("body", "{}")`, which previously returned the literal string `"{}"` for non-string bodies (t10 flake root cause). Originally committed as 5198104; bundled into 11c6642 via `git add -A` race during dev-0f46's push.

### Fixed
- **TD-97** (08f9f97): `handleLogin` now busy-waits up to `handlerWaitMs` (default 2000ms, override via `SEMANTIC_HANDLER_WAIT_MS` env) for `SemanticAgent.loginHandler` to register before returning 503. Closes the cold-init race where `ContentProvider.onCreate` (which calls `SemanticServer.install` → `server.start()`) runs before `Application.onCreate` (which registers the login handler), leaving a ~100s-of-ms window where `/login` requests fail with `"no loginHandler registered"`. Smallest blast-radius option: no consumer migration, no public API change, no init-sequence change.

### Notes
- v0.2.0 covered Epic L doctrine codification (release discipline); this 0.3.0 closes the post-Epic L flake-drill follow-through that the 7-TC bucket synth surfaced.
- All three Android flake-drill buckets (Epic-C-v2-WILL-FIX: t7/t15/t25; ORTHOGONAL: t8/t10/t12; dispatch-ordering 4th-bucket: t34) now have shipped fixes; variance×3 verification in flight.
