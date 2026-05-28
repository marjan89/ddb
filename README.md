# Semantic Agent — Android

Embeddable debug-only HTTP server that exposes the Android view tree as structured YAML for automated QA tooling. Runs on port 9876 inside the app process.

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | /health | Agent identity + version |
| GET | /version | Git hash + build time |
| GET | /idle | Layout + scroll idle state |
| GET | /semantic | Full view tree as YAML |
| GET | /stream | SSE event stream (activity, idle) |
| GET | /permissions | App permission status |
| POST | /auth/login | Login via AgentAuth |
| POST | /auth/logout | Logout via AgentAuth |
| GET | /auth/state | Auth status + user ID |
| POST | /navigate/site/{id} | Navigate to site detail |
| POST | /navigate/user/{id} | Navigate to user profile |
| POST | /keyboard/dismiss | Dismiss soft keyboard |
| POST | /state/reset | Clear app state |
| DELETE | /question/{id} | Delete a question |

## Integration

### 1. Add dependency (debug only)

```kotlin
// app/build.gradle.kts
debugImplementation(project(":semantic-agent"))
// or: debugImplementation("dev.substrate:semantic-agent:1.0.0")
```

### 2. Implement interfaces

```kotlin
// AgentNavigator — how to navigate to screens
class MyNavigator : AgentNavigator {
    override fun createSiteIntent(activity: Activity, siteId: Int): Intent =
        SiteDetailActivity.newIntent(activity, siteId)

    override fun createUserIntent(activity: Activity, userId: Int): Intent =
        UserProfileActivity.newIntent(activity, userId)
}

// AgentAuth — how to authenticate
class MyAuth(
    private val remoteDataSource: RemoteDataSource,
    private val userRepository: UserRepository,
) : AgentAuth {
    override suspend fun login(email: String, password: String): Result<Unit> { /* ... */ }
    override suspend fun logout() { /* ... */ }
    override suspend fun isAuthenticated(): Boolean { /* ... */ }
    override suspend fun getUserId(): Int { /* ... */ }
    override suspend fun resetState() { /* ... */ }
    override suspend fun deleteQuestion(questionId: Int): Boolean { /* ... */ }
}
```

### 3. Initialize in debug ContentProvider

```kotlin
class SemanticInitProvider : ContentProvider() {
    override fun onCreate(): Boolean {
        val app = context?.applicationContext as? Application ?: return false
        SemanticServer.install(
            app,
            gitHash = BuildConfig.GIT_HASH,
            buildTime = BuildConfig.BUILD_TIME,
            navigator = MyNavigator(),
            auth = MyAuth(/* inject deps */),
        )
        return true
    }
    // ... stub methods
}
```

Register in `src/debug/AndroidManifest.xml`:
```xml
<provider
    android:name=".SemanticInitProvider"
    android:authorities="${applicationId}.semantic"
    android:exported="false" />
```

### 4. Port forwarding

```bash
adb forward tcp:9876 tcp:9876
curl http://localhost:9876/health
# {"status":"ok","agent":"semantic-agent","version":"5.0.0"}
```

## Architecture

- **SemanticServer.kt** — NanoHTTPD server, route dispatch, SSE stream
- **ViewTreeWalker.kt** — Recursive view tree → SemanticElement conversion
- **SemanticElement.kt** — Data model for UI elements (bounds, text, clickable, etc.)
- **AgentContracts.kt** — AgentAuth + AgentNavigator interfaces

Zero app-specific imports in engine code. All app integration via interfaces.

## Used by

- [ddb](../device-control-android) — Android test runner
- [vdb](../visual-debug-bridge) — Visual diff + semantic compare
