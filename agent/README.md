# semantic-agent (Android)

Debug-only in-process HTTP server exposing the Android view tree, idle state, and tap/type surface for automated QA. Released as `dev.substrate:semantic-agent`.

See [tctl/docs/agent-porting-guide.md](../tctl/docs/agent-porting-guide.md) for the cross-platform contract and [tctl/docs/agent-capability-matrix.md](../tctl/docs/agent-capability-matrix.md) for per-platform feature status.

## Versions

| Version | SHA | Change |
|---|---|---|
| 0.4.0 | 25b493b | `POST /text-field/set` atomic value replace |
| 0.3.0 | 7c8f517 | `IdleResourceRegistry.safeIsIdle` throwable guard |
| 0.2.0 | 88b7f6f | public `SemanticServer.registerIdleResource(name, lambda)` |
| 0.1.0 | b57401c | initial publish |

## Consume

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories { mavenLocal() }
}

// app/build.gradle.kts
dependencies {
    debugImplementation("dev.substrate:semantic-agent:0.4.0")
}
```

`SemanticInitProvider` auto-starts the server. No `Application.onCreate` boilerplate required for the agent itself.

## Endpoints

| Method | Path | Notes |
|---|---|---|
| GET  | `/health` | `{"status":"ok","agent":"semantic-agent","version":"3.0.0"}` |
| GET  | `/version` | git hash + build time when packaged with `gitHash`/`buildTime` to `install()` |
| GET  | `/semantic` | YAML element list, current screen |
| GET  | `/semantic?scroll=0` | full-page semantic, ignores viewport |
| GET  | `/idle` | `{"idle":bool}` — AND of every registered resource (throwable-guarded since 0.3.0) |
| GET  | `/idle-resources` | list registered resource names |
| POST | `/query-when-idle` | wait for idle + element match, return coords |
| POST | `/scroll-search` | scroll within a scrollable to surface an off-screen element |
| POST | `/click` | tap by `{resource_id|content_fuzzy|bounds}` |
| POST | `/type` | per-char `commitText` via `InputConnection` — see caveat below |
| POST | `/text-field/set` (0.4.0+) | atomic `setText` on focused `EditText`, single TextWatcher fire; preferred for login + IME-sensitive paths |
| POST | `/keyboard/dismiss` | `imm.hideSoftInputFromWindow` |
| POST | `/mock` | register mock HTTP rules |
| POST | `/unmock` | clear mock rules |
| GET  | `/mock-status` | hit counts + registered rules |
| GET  | `/stream` | SSE event stream |
| GET  | `/overlay` | optional debug overlay |
| GET  | `/debug-log` | recent agent log |
| DELETE | `/debug-log` | clear |

## Idle resource registration

Built-in resources at `SemanticServer.install()`: `ui_thread`, `layout`, `scroll`, `network`, `dialog`, `activity_transition`.

`network` uses reflection to find a Hilt-provided OkHttp `Dispatcher`. Apps that don't match that shape (different DI, no Hilt, custom HTTP client) MUST register explicitly:

```kotlin
val lastStart = java.util.concurrent.atomic.AtomicLong(System.currentTimeMillis())
okHttpClientBuilder.eventListener(object : okhttp3.EventListener() {
    override fun requestHeadersStart(call: okhttp3.Call) {
        lastStart.set(System.currentTimeMillis())
    }
})
SemanticServer.registerIdleResource("okhttp") {
    val sinceLast = System.currentTimeMillis() - lastStart.get()
    dispatcher.runningCallsCount() == 0 && sinceLast > 1500
}
```

EventListener (not `addInterceptor`) is required so the listener doesn't fire for mocked requests (mock chain short-circuits inside the interceptor stack). 1.5s settle prevents background polling (workers, analytics) from holding `/idle` busy.

Reference integration: nk-android-2026 `app/src/main/java/se/naturkartan/android/di/Module.kt` (c7dc2eb) + eager `@Inject RestApi` in `MainApplication.onCreate` so the provider runs before the agent's first `/idle` probe.

## Publish

```
nosandbox ./gradlew :agent:publishReleasePublicationToMavenLocal --no-daemon
```

Publishes to `~/.m2/repository/dev/substrate/semantic-agent/<version>/`.

## Endpoint selection guidance

| Use case | Endpoint | Why |
|---|---|---|
| Plain text input, non-secure field | `/type` | per-char keystrokes acceptable; fires standard text events |
| Password, SecureKeyboard, autofill-prone field | `/text-field/set` | atomic, single TextWatcher fire, bypasses IME/autofill races |
| Tap addressable element | `/click` with `resource_id` | most stable |
| Tap text label | `/click` with `content_fuzzy` | fuzzy substring match |
| Tap unlabeled touch zone | `/click` with `bounds` | last resort |

## Known limitations

See [tctl/docs/agent-capability-matrix.md](../tctl/docs/agent-capability-matrix.md) §"Known limitations" rows L2, L5, L6, L7, L8, L9.
