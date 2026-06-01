# Semantic Agent — Android

Embeddable debug-only HTTP server that exposes the Android view tree as structured YAML for automated QA tooling. Runs on port 9876 inside the app process.

## Quick Start

### 1. Add dependency

```gradle
// settings.gradle.kts — add Maven Local
repositories {
    mavenLocal()
}

// app/build.gradle.kts
dependencies {
    debugImplementation("dev.substrate:semantic-agent:0.1.0")
}
```

### 2. Wire MockInterceptor into OkHttp (optional, for mock layer)

```kotlin
// In your DI module (debug build only)
if (BuildConfig.DEBUG) {
    val registry = dev.substrate.semantic.MockRegistry.shared
    okHttpClientBuilder.addInterceptor(registry.interceptor)
}
```

### 3. Auto-start

The agent starts automatically via `SemanticInitProvider` (ContentProvider). No code needed — just add the dependency.

### 4. Verify

```bash
adb forward tcp:9876 tcp:9876
curl http://127.0.0.1:9876/health
# → {"status":"ok","agent":"semantic-agent"}
curl http://127.0.0.1:9876/semantic
# → YAML view tree
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | /health | Health check |
| GET | /version | Agent version (git hash + build time) |
| GET | /semantic | View tree as YAML |
| GET | /idle | Idle resource status |
| POST | /query-when-idle | Wait for idle then return semantic |
| POST | /scroll-search | Scroll + search for element |
| POST | /type | Type text into focused/targeted field (clear + InputConnection) |
| POST | /click | Programmatic performClick by resource_id |
| POST | /mock | Register mock HTTP rules |
| POST | /unmock | Clear mock rules |
| GET | /mock-status | Mock hit count and registered rules |
| POST | /keyboard/dismiss | Dismiss software keyboard |

## Publishing to Maven Local

```bash
cd semantic-agent-android
./gradlew :agent:publishToMavenLocal
```

Publishes `dev.substrate:semantic-agent:0.1.0` to `~/.m2/repository/`.

## Configuration

The agent accepts constructor parameters with defaults:

| Parameter | Default | Description |
|-----------|---------|-------------|
| port | 9876 | HTTP server port |

## Architecture

- **SemanticServer**: NanoHTTPD-based HTTP server, routes requests to handlers
- **ViewTreeWalker**: Walks Android view hierarchy, produces SemanticElement tree
- **SemanticElement**: Structured representation of a UI element (text, bounds, type, resource_id)
- **IdleResourceRegistry**: Tracks UI idle state (network, animations, UI thread)
- **MockRegistry + MockInterceptor**: OkHttp interceptor for HTTP mocking
- **SemanticInitProvider**: ContentProvider for auto-start (no app code needed)
