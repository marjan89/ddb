# Android Semantic Agent

An in-process HTTP server injected into debug Android builds that provides rich semantic extraction of the live view hierarchy. Runs on port 9876 and serves YAML/JSON responses to ddb, vdb, and other tooling. Replaces uiautomator for semantic extraction with higher-fidelity data including font metrics, colors, accessibility labels, margins, elevation, and image resources.

## Install

Mount the agent into an Android project (no source code changes required):

```bash
ddb mount semantic --project /path/to/android-project
cd /path/to/android-project
./gradlew assembleStandardDebug
ddb app deploy /tmp/app-debug.apk
```

Remove:
```bash
ddb unmount semantic --project /path/to/android-project
./gradlew assembleStandardDebug
```

## What Gets Injected

| File | Location | Purpose |
|------|----------|---------|
| `SemanticInitProvider.kt` | `app/src/debug/java/dev/substrate/semantic/` | ContentProvider for auto-initialization (initOrder: 99) |
| `SemanticServer.kt` | same | NanoHTTPD server on port 9876 with API endpoints |
| `ViewTreeWalker.kt` | same | Depth-first view hierarchy traversal |
| `SemanticElement.kt` | same | Data models for extracted elements |
| `AndroidManifest.xml` | `app/src/debug/` | ContentProvider registration + INTERNET permission |
| `semantic-agent.gradle.kts` | `.gradle/init.d/` | Adds `debugImplementation("org.nanohttpd:nanohttpd:2.3.1")` |

All files are debug-only. Release builds contain no agent code.

## Activation

Automatic. `SemanticInitProvider` is a ContentProvider with `initOrder: 99` that calls `SemanticServer.install(app)` during app startup. No code changes, no manual initialization. The server registers `ActivityLifecycleCallbacks` to track the current foreground activity via WeakReference.

## Endpoints

### GET /semantic

Extract the semantic tree from the current activity.

```bash
curl http://localhost:9876/semantic
curl "http://localhost:9876/semantic?scroll=true"
curl "http://localhost:9876/semantic?scroll=5"
```

| Param | Description |
|-------|-------------|
| `scroll=true` | Enable scroll-capture mode (auto-detect step count) |
| `scroll=<N>` | Scroll-capture with N steps |

Returns YAML semantic schema. Timeout: 5s (30s for scroll capture).

**Response codes:** 200 OK, 503 no active activity, 500 internal error.

### GET /overlay | DELETE /overlay

Visual debugging overlay that renders colored bounding boxes over detected elements.

```bash
curl "http://localhost:9876/overlay?mode=stroke"   # white background + colored borders
curl "http://localhost:9876/overlay?mode=fill"      # filled color boxes
curl -X DELETE http://localhost:9876/overlay        # remove overlay
```

Uses DJB2 hash of element ID for consistent per-element colors.

### GET /debug-log

View the debug log from the most recent view tree walk.

```bash
curl http://localhost:9876/debug-log
```

### GET /health

Health check.

```bash
curl http://localhost:9876/health
# {"status":"ok","agent":"semantic-agent","version":"3.0.0"}
```

## Walker Algorithm

`ViewTreeWalker.walk()` performs depth-first recursive traversal starting from `activity.window.decorView`.

**Filtering (nodes skipped):**
1. GONE visibility
2. INVISIBLE or alpha=0 AND is a leaf node
3. No visible rect on screen (`getGlobalVisibleRect()` returns false)
4. Zero or negative bounds dimensions

**External surface detection:** MapView, SurfaceView, TextureView, VideoView, WebView, GLSurfaceView, and ExoPlayerView are marked with `render: "external"` — their content is rendered outside the view tree and cannot be inspected.

**Ghost touch target removal:** Post-processing filter removes empty clickable views that wrap visible child elements with content (common in Material ripple containers).

**Z-index:** Linear sequential numbering in depth-first traversal order.

## Enriched Fields

The agent extracts significantly more data than uiautomator:

### Identity
| Field | Source |
|-------|--------|
| `id` | Slugified from content, resource ID, or position |
| `platformId` | Android resource entry name (`view.resources.getResourceEntryName()`) |
| `type` | "button", "text", "input", "image", "container", "view" |

### Content
| Field | Source |
|-------|--------|
| `content` | `TextView.text`, `EditText.text`, `contentDescription`, or `hint` |
| `lineCount` | `TextView.lineCount` |
| `truncated` | `TextView.layout.getEllipsisCount() > 0` |
| `a11yLabel` | `contentDescription` |

### Typography
| Field | Source |
|-------|--------|
| `font.family` | `Typeface` family name (cleaned of `@font/`, resource prefixes) |
| `font.weight` | Mapped from typeface style: thin/extralight/light/regular/medium/semibold/bold/extrabold/black |
| `font.size` | `textSize` converted to DP |

### Colors
| Field | Source |
|-------|--------|
| `color` | Foreground text color as `#RRGGBB` or `#AARRGGBB` |
| `background` | From ColorDrawable, GradientDrawable, or RippleDrawable |

### Layout
| Field | Source |
|-------|--------|
| `bounds.x/y/w/h` | `getGlobalVisibleRect()` converted to DP |
| `zIndex` | Sequential depth-first order |
| `elevation` | `elevation + translationZ` in DP |
| `padding` | top/bottom/start/end in DP |
| `margin` | top/bottom/start/end in DP (from LayoutParams) |
| `cornerRadius` | From GradientDrawable corner radius in DP |

### State
| Field | Source |
|-------|--------|
| `clickable` | `view.isClickable` |
| `enabled` | `view.isEnabled` |
| `accessible` | `view.isImportantForAccessibility` |

### Images
| Field | Source |
|-------|--------|
| `imageResource` | Drawable resource name or ID |
| `imageType` | "vector", "raster", or "loaded" (Glide/network) |
| `imagePath` | Saved PNG path in `cache/vdb-images/` (max 256x256) |

### Metadata
| Field | Source |
|-------|--------|
| `screen` | Activity class name |
| `device` | `Build.MODEL` |
| `platform` | "android" |
| `timestamp` | ISO 8601 UTC |
| `viewport.width/height` | Screen dimensions in DP |
| `viewport.density` | Display density factor |

## Scroll Capture

When `scroll=true` or `scroll=N` is passed to `/semantic`, the agent:

1. Finds the scrollable container (RecyclerView, ScrollView, NestedScrollView)
2. Captures the initial viewport
3. Programmatically scrolls and captures elements at each position
4. Adjusts bounds with cumulative scroll offset
5. Detects sticky elements (position unchanged during scroll)
6. Deduplicates elements within +/-3 DP tolerance
7. Returns the combined element list with scroll metadata

## Examples

**Extract semantic tree via ddb:**
```bash
ddb ui --semantic -o /tmp/android-home.yaml
```

**Direct curl for scripting:**
```bash
curl -s http://localhost:9876/semantic > /tmp/semantic.yaml
```

**Scroll-capture a long screen:**
```bash
curl -s "http://localhost:9876/semantic?scroll=10" > /tmp/full-page.yaml
```

**Cross-platform comparison pipeline:**
```bash
ddb ui --semantic -o /tmp/android-site.yaml
fdb convert --kiwi -i /tmp/scenegraph.json --frame "Site" -o /tmp/figma-site.yaml
vdb diff /tmp/figma-site.yaml /tmp/android-site.yaml
```
