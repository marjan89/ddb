# ADR-001: Idle Barrier Over Polling

## Status: Accepted

## Context

The ddb runner originally polled for elements using 10 iterations × 1s sleep = up to 90s for a failing assert. Each poll made 3 network calls (uiautomator dump, dumpsys activity top, semantic agent) at ~5-15s each. Failing assertions wasted 90+ seconds.

The semantic agent on Android has SSE via `/stream` that emits events on activity lifecycle changes, scroll state changes, and idle state. iOS agent has `/idle` endpoint but not SSE (yet).

Industry: Detox uses an idle-resource barrier — the runner doesn't poll, it waits for the app to signal "idle" then queries once. Espresso uses IdlingResource for the same pattern. Appium recommends explicit waits only, never implicit.

## Decision

Replace polling with SSE-first element detection:
1. Quick check (one pass through all sources)
2. Subscribe to `/stream` SSE with 10s timeout
3. On each SSE event, re-check all sources
4. After timeout, final check then FAIL

Phase 5 target: `POST /query-when-idle` on the semantic agent — the agent waits for idle internally and returns the result in one round-trip.

## Consequences

- Failing asserts complete in ~10s instead of ~90s
- Passing asserts complete on first check (0s wait) or on first relevant SSE event (~1-2s)
- Requires SSE on iOS agent (Phase 3 implements this)
- Falls back to quick check if SSE unavailable
