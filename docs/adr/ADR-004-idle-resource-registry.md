# ADR-004: Semantic Agent as In-App Idle Resource Registry

## Status
Proposed

## Context
The test runner polls GET /idle to detect when the app is ready for interaction. This polling wastes ~5s per wait on a 200ms interval. The Espresso framework solves this with IdlingResource — a registry where async operations register themselves, and the framework waits until all are idle.

The semantic agent already lives inside the app process. It has access to ViewTreeObserver, RecyclerView scroll state, and the main thread handler. It can implement the same pattern as Espresso's IdlingResource without requiring Espresso as a dependency.

## Decision
The semantic agent becomes the idle resource registry for the app. Components register idle conditions:

1. **Layout idle**: ViewTreeObserver.OnGlobalLayoutListener — no pending layout passes
2. **Scroll idle**: RecyclerView.SCROLL_STATE_IDLE on all visible RecyclerViews
3. **Network idle**: OkHttp dispatcher has 0 running calls (optional, via interceptor)
4. **Animation idle**: ViewPropertyAnimator completion callbacks

The /idle endpoint returns true only when ALL registered resources report idle. The /stream SSE endpoint emits an "idle" event when the state transitions from busy→idle.

Future: POST /query-when-idle blocks until idle, then walks the view tree and returns the semantic YAML in one roundtrip — eliminating the poll→check→fetch triple.

## Consequences
- Eliminates polling for idle detection (polling replaced by SSE push)
- Runners subscribe to /stream for "idle" event instead of GET /idle loop
- Custom idle resources can be registered by the app (e.g., "data loading complete")
- POST /query-when-idle combines wait+fetch into one HTTP call
- Aligns with Espresso's proven IdlingResource pattern without the Espresso dependency
