# Changelog

All notable changes to semantic-agent-android are documented here.

## [v0.4.0] — 2026-06-09

### Fixed
- **TD-101 Bug 2 — handleLogin idle barrier** (bf88b13): closes part (b) of the TD-70 comment intent that never landed. After the loginHandler callback completes (the only signal `handleLogin` gets that the success path ran) and after `cachedSchema = null`, run a main-thread coroutine looping `idleRegistry.isIdle()` + `awaitFrame()` up to 30 frames (~500ms ceiling), latch-bounded to 3s, before returning 200. Without this, the recipe's next `/semantic` poll races the Compose recompose triggered by the handler's state mutation and reads the pre-login tree (t34 indicator-poll flake). `handleSemantic`'s own `awaitFrame` loop only gates on scroll-idle, which exits at i=0 on a static screen — the barrier has to live in `handleLogin` because that's the only point at which we know a state mutation just happened and needs to surface. Pairs with regression-android v0.3.0 Bug 1 (main-thread marshal of T13Store mutation). Fresh-APK variance×3 after both: **81/81 deterministic**.
- **TD-93 `/debug-reset` endpoint + `SEMANTIC_DEBUG` gate** (9b93cf4): defensive depth for the JVM-survives path. `handleDebugReset` clears `mockRegistry`, `eventQueue`, `requestLog`, `cachedSchema` and returns `{"reset":true}`. Off by default; the host process must set `SEMANTIC_DEBUG=1` in its env to enable. Recipe runners that can't or won't `am force-stop` between TCs can wipe agent singleton state without restarting the process. The primary TD-93 fix lives in the regression-android wrapper (`RESET_MODE=am-restart` default); this is the inner-layer fallback.
- **TD-97 `handlerWaitMs` default 2000 → 5000** (076b39e): once `RESET_MODE=am-restart` (TD-93) became default and every TC pays a full process re-fork from zygote, the 2s busy-wait budget sized for warm-process cold-init no longer fit full-fork cold-init on emulators. The bounded wait still costs only when the handler is genuinely null; warm-process callers return on the first read. `SEMANTIC_HANDLER_WAIT_MS` env override preserved for slower hardware (drill ceiling 8000).

### Notes
- v0.3.0 closed the Epic C v2 / 7-TC flake-drill bucket synth (TD-91/92/95/59 + TD-97 v1). v0.4.0 closes the residual t34 indicator-poll race (TD-101) and tightens TD-93 + TD-97 with the lessons from the verify-sweep cycle.
- Combined with regression-android v0.3.0 (RESET_MODE flip + Bug 1 main-thread marshal + init-script relocation), the Android regression harness reaches the long-standing 81/81 deterministic target across r=1/r=2/r=3 of variance×3.

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
