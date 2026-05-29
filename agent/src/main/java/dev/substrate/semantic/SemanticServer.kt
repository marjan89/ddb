package dev.substrate.semantic

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import fi.iki.elonen.NanoHTTPD
import kotlinx.coroutines.runBlocking
import java.lang.ref.WeakReference

class SemanticServer private constructor(
    port: Int,
    private val gitHash: String = "",
    private val buildTime: String = "",
) : NanoHTTPD(port) {

    private var currentActivity: WeakReference<Activity>? = null
    private var appRef: WeakReference<Application>? = null
    private val mainHandler = Handler(Looper.getMainLooper())
    @Volatile private var cachedSchema: SemanticSchema? = null
    private var overlayView: android.view.View? = null
    private var navigator: AgentNavigator? = null
    private var auth: AgentAuth? = null
    val idleRegistry = IdleResourceRegistry()

    private data class RequestLogEntry(
        val ts: Long,
        val method: String,
        val path: String,
        val status: Int,
        val durationMs: Long,
        val bodySize: Int,
    )

    private val requestLog = java.util.concurrent.ConcurrentLinkedDeque<RequestLogEntry>()
    private val maxLogEntries = 100

    private fun logRequest(entry: RequestLogEntry) {
        requestLog.addLast(entry)
        while (requestLog.size > maxLogEntries) requestLog.pollFirst()
    }

    override fun serve(session: IHTTPSession): Response {
        val uri = session.uri.trimEnd('/')
        val startMs = System.currentTimeMillis()
        val method = session.method.name

        val response = serveInner(session, uri)

        val durationMs = System.currentTimeMillis() - startMs
        val status = response.status.requestStatus
        val bodySize = response.data?.available() ?: 0
        logRequest(RequestLogEntry(startMs, method, uri, status, durationMs, bodySize))
        android.util.Log.d("SemanticAgent", "$method $uri → $status (${durationMs}ms)")

        return response
    }

    private fun serveInner(session: IHTTPSession, uri: String): Response {
        return when {
            uri == "/semantic" -> {
                val scrollParam = session.parms?.get("scroll")
                val scrollSteps = scrollParam?.toIntOrNull()
                    ?: if (scrollParam?.toBooleanStrictOrNull() == true) 5 else 0
                if (scrollSteps > 0) handleSemanticScroll(scrollSteps) else handleSemantic()
            }
            uri == "/overlay" -> {
                if (session.method == Method.DELETE) handleOverlayOff()
                else handleOverlayOn(session)
            }
            uri == "/debug-log" && session.method == Method.DELETE -> { requestLog.clear(); jsonResponse("""{"cleared":true}""") }
            uri == "/debug-log" -> handleDebugLog()
            uri == "/health" -> jsonResponse("""{"status":"ok","agent":"semantic-agent","version":"5.0.0"}""")
            uri == "/version" -> jsonResponse("""{"git_hash":"$gitHash","build_time":"$buildTime"}""")
            uri == "/idle" -> handleIdle()
            uri == "/auth/login" && session.method == Method.POST -> handleAuthLogin(session)
            uri == "/auth/logout" && session.method == Method.POST -> handleAuthLogout()
            uri == "/auth/state" -> handleAuthState()
            uri == "/state/reset" && session.method == Method.POST -> handleStateReset()
            uri == "/permissions" -> handlePermissions()
            uri.startsWith("/navigate/site/") -> handleNavigateSite(uri)
            uri.startsWith("/navigate/user/") -> handleNavigateUser(uri)
            uri == "/keyboard/dismiss" && session.method == Method.POST -> handleKeyboardDismiss()
            uri.startsWith("/question/") && session.method == Method.DELETE -> handleDeleteQuestion(uri)
            uri == "/stream" -> handleStream()
            uri == "/query-when-idle" && session.method == Method.POST -> handleQueryWhenIdle(session)
            uri == "/scroll-search" && session.method == Method.POST -> handleScrollSearch(session)
            uri == "/idle-resources" -> handleIdleResources()
            else -> newFixedLengthResponse(Response.Status.NOT_FOUND, "text/plain", "not found")
        }
    }

    private fun handleIdle(): Response {
        val activity = currentActivity?.get()
            ?: return jsonResponse("""{"idle":true}""")
        var idle = true
        val latch = java.util.concurrent.CountDownLatch(1)
        mainHandler.post {
            val rootView = activity.window.decorView
            val layoutIdle = !rootView.isLayoutRequested && !rootView.isDirty
            val scrollIdle = isScrollIdle(rootView)
            idle = layoutIdle && scrollIdle
            latch.countDown()
        }
        if (!latch.await(2, java.util.concurrent.TimeUnit.SECONDS)) {
            return jsonResponse("""{"idle":false,"reason":"timeout"}""")
        }
        return jsonResponse("""{"idle":$idle}""")
    }

    internal fun isScrollIdle(view: android.view.View): Boolean {
        if (view.javaClass.name.contains("RecyclerView")) {
            try {
                val m = view.javaClass.getMethod("getScrollState")
                val state = m.invoke(view) as Int
                if (state != 0) return false // SCROLL_STATE_IDLE = 0
            } catch (_: Exception) {}
        }
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                if (!isScrollIdle(view.getChildAt(i))) return false
            }
        }
        return true
    }

    private fun handleAuthLogin(session: IHTTPSession): Response {
        val agentAuth = auth ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "app not ready")
        val contentLength = session.headers["content-length"]?.toIntOrNull() ?: 0
        val body = ByteArray(contentLength)
        session.inputStream.read(body, 0, contentLength)
        val json = String(body)
        val email = extractJsonString(json, "email")
        val password = extractJsonString(json, "password")
        if (email == null || password == null) {
            return jsonResponse("""{"error":"missing email or password"}""", Response.Status.BAD_REQUEST)
        }
        return runBlocking {
            val result = agentAuth.login(email, password)
            if (result.isSuccess) {
                jsonResponse("""{"logged_in":true}""")
            } else {
                jsonResponse("""{"logged_in":false,"error":"invalid credentials"}""", Response.Status.UNAUTHORIZED)
            }
        }
    }

    private fun handleAuthLogout(): Response {
        val agentAuth = auth ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "app not ready")
        return runBlocking {
            agentAuth.logout()
            jsonResponse("""{"logged_in":false}""")
        }
    }

    private fun handleAuthState(): Response {
        val agentAuth = auth ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "app not ready")
        return runBlocking {
            val authenticated = agentAuth.isAuthenticated()
            val userId = agentAuth.getUserId()
            jsonResponse("""{"logged_in":$authenticated,"user_id":$userId}""")
        }
    }

    private fun handleStateReset(): Response {
        val agentAuth = auth ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "app not ready")
        return runBlocking {
            agentAuth.resetState()
            jsonResponse("""{"reset":true}""")
        }
    }

    private fun handlePermissions(): Response {
        val app = appRef?.get() ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "app not ready")
        val pm = app.packageManager
        val pkgInfo = pm.getPackageInfo(app.packageName, PackageManager.GET_PERMISSIONS)
        val requested = pkgInfo.requestedPermissions ?: emptyArray()
        val flags = pkgInfo.requestedPermissionsFlags
        val granted = if (flags != null) {
            requested.filterIndexed { i, _ -> i < flags.size && flags[i] and PackageManager.PERMISSION_GRANTED != 0 }
        } else emptyList()
        val items = requested.map { perm ->
            val isGranted = granted.contains(perm)
            """{"permission":"$perm","granted":$isGranted}"""
        }
        return jsonResponse("""{"package":"${app.packageName}","permissions":[${items.joinToString(",")}]}""")
    }

    private fun handleNavigateSite(uri: String): Response {
        val siteId = uri.removePrefix("/navigate/site/").toIntOrNull()
            ?: return jsonResponse("""{"error":"invalid site id"}""", Response.Status.BAD_REQUEST)
        val activity = currentActivity?.get()
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "no active activity")
        val nav = navigator
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "navigator not set")
        mainHandler.post {
            val intent = nav.createSiteIntent(activity, siteId)
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(intent)
        }
        return jsonResponse("""{"navigated":"site","id":$siteId}""")
    }

    private fun handleNavigateUser(uri: String): Response {
        val userId = uri.removePrefix("/navigate/user/").toIntOrNull()
            ?: return jsonResponse("""{"error":"invalid user id"}""", Response.Status.BAD_REQUEST)
        val activity = currentActivity?.get()
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "no active activity")
        val nav = navigator
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "navigator not set")
        mainHandler.post {
            val intent = nav.createUserIntent(activity, userId)
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(intent)
        }
        return jsonResponse("""{"navigated":"user","id":$userId}""")
    }

    private fun handleKeyboardDismiss(): Response {
        val activity = currentActivity?.get()
            ?: return jsonResponse("""{"dismissed":false}""")
        val latch = java.util.concurrent.CountDownLatch(1)
        var dismissed = false
        mainHandler.post {
            val imm = activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE) as android.view.inputmethod.InputMethodManager
            val view = activity.currentFocus ?: activity.window.decorView
            dismissed = imm.hideSoftInputFromWindow(view.windowToken, 0)
            latch.countDown()
        }
        latch.await(2, java.util.concurrent.TimeUnit.SECONDS)
        return jsonResponse("""{"dismissed":$dismissed}""")
    }

    private val eventQueue = java.util.concurrent.LinkedBlockingQueue<String>()

    fun emitEvent(event: String, data: String) {
        eventQueue.offer("event: $event\ndata: $data\n\n")
    }

    private fun handleStream(): Response {
        val stream = object : java.io.InputStream() {
            private var buffer = ByteArray(0)
            private var pos = 0

            override fun read(): Int {
                while (true) {
                    if (pos < buffer.size) return buffer[pos++].toInt() and 0xFF
                    val msg = eventQueue.poll(30, java.util.concurrent.TimeUnit.SECONDS)
                        ?: return -1
                    buffer = msg.toByteArray()
                    pos = 0
                }
            }
        }
        return newChunkedResponse(Response.Status.OK, "text/event-stream", stream).apply {
            addHeader("Cache-Control", "no-cache")
            addHeader("Connection", "keep-alive")
        }
    }

    private fun handleDeleteQuestion(uri: String): Response {
        val questionId = uri.removePrefix("/question/").trimEnd('/')
        val qId = questionId.toIntOrNull()
        if (qId == null) {
            return jsonResponse("""{"error":"invalid question id"}""", Response.Status.BAD_REQUEST)
        }
        val agentAuth = auth ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "app not ready")
        return runBlocking {
            val success = agentAuth.deleteQuestion(qId)
            if (success) {
                jsonResponse("""{"deleted":true,"id":$qId}""")
            } else {
                jsonResponse("""{"deleted":false,"error":"delete failed"}""", Response.Status.INTERNAL_ERROR)
            }
        }
    }

    private fun extractJsonString(json: String, key: String): String? {
        val pattern = """"$key"\s*:\s*"([^"\\]*(?:\\.[^"\\]*)*)"""".toRegex()
        return pattern.find(json)?.groupValues?.get(1)
    }

    private fun jsonResponse(json: String, status: Response.Status = Response.Status.OK): Response {
        return newFixedLengthResponse(status, "application/json", json)
    }

    private fun handleSemantic(): Response {
        val activity = currentActivity?.get()
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "no active activity")

        var schema: SemanticSchema? = null
        var error: String? = null
        val latch = java.util.concurrent.CountDownLatch(1)

        mainHandler.post {
            try {
                val rootView = activity.window.decorView
                for (i in 0..10) {
                    if (isScrollIdle(rootView)) break
                    Thread.sleep(100)
                }
                schema = ViewTreeWalker.walk(activity)
                cachedSchema = schema
            } catch (e: Exception) {
                error = e.message ?: "unknown error"
            }
            latch.countDown()
        }

        if (!latch.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
            return newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "text/plain", "timeout walking view tree")
        }

        if (error != null) {
            return newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "text/plain", "error: $error")
        }

        return newFixedLengthResponse(Response.Status.OK, "text/yaml", toYaml(schema!!))
    }

    private fun handleSemanticScroll(steps: Int): Response {
        val activity = currentActivity?.get()
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "no active activity")

        var result: SemanticSchema? = null
        var error: String? = null
        val latch = java.util.concurrent.CountDownLatch(1)

        mainHandler.post {
            try {
                result = walkWithScroll(activity, steps)
                cachedSchema = result
            } catch (e: Exception) {
                error = e.stackTraceToString()
            }
            latch.countDown()
        }

        if (!latch.await(30, java.util.concurrent.TimeUnit.SECONDS)) {
            return newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "text/plain", "timeout during scroll walk")
        }

        if (error != null) {
            return newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "text/plain", "error: $error")
        }

        return newFixedLengthResponse(Response.Status.OK, "text/yaml", toYaml(result!!))
    }

    private fun walkWithScroll(activity: Activity, steps: Int): SemanticSchema {
        val scrollable = findScrollable(activity.window.decorView)
            ?: throw IllegalStateException("no scrollable view found")

        val density = activity.resources.displayMetrics.density
        val viewportH = (activity.resources.displayMetrics.heightPixels / density).toInt()
        val scrollAmount = (viewportH * 0.7).toInt()
        val scrollAmountPx = (scrollAmount * density).toInt()

        val svRect = android.graphics.Rect()
        scrollable.getGlobalVisibleRect(svRect)
        val scrollViewBounds = Bounds(
            x = (svRect.left / density).toInt(),
            y = (svRect.top / density).toInt(),
            w = ((svRect.right - svRect.left) / density).toInt(),
            h = ((svRect.bottom - svRect.top) / density).toInt(),
        )

        val allElements = mutableListOf<SemanticElement>()
        val stickyIds = mutableSetOf<String>()
        var cumulativeScrollDp = 0
        val stepOffsets = mutableListOf(0)

        val firstSchema = ViewTreeWalker.walk(activity)
        val firstElements = firstSchema.elements
        allElements.addAll(firstElements)

        for (step in 1..steps) {
            scrollable.scrollBy(0, scrollAmountPx)

            try { Thread.sleep(300) } catch (_: InterruptedException) {}

            val schema = ViewTreeWalker.walk(activity)
            val newElements = schema.elements
            cumulativeScrollDp += scrollAmount
            stepOffsets.add(cumulativeScrollDp)

            if (step == 1) {
                for (e in newElements) {
                    val match = firstElements.find { it.type == e.type && it.content == e.content }
                    if (match != null && match.bounds == e.bounds) {
                        stickyIds.add(boundsKey(e))
                    }
                }
            }

            for (e in newElements) {
                val key = boundsKey(e)
                if (stickyIds.contains(key)) continue

                val adjusted = e.copy(
                    bounds = Bounds(
                        x = e.bounds.x,
                        y = e.bounds.y + cumulativeScrollDp,
                        w = e.bounds.w,
                        h = e.bounds.h,
                    ),
                )

                val isDuplicate = allElements.any { existing ->
                    existing.type == adjusted.type
                        && existing.content == adjusted.content
                        && kotlin.math.abs(existing.bounds.x - adjusted.bounds.x) < 3
                        && kotlin.math.abs(existing.bounds.y - adjusted.bounds.y) < 3
                        && kotlin.math.abs(existing.bounds.w - adjusted.bounds.w) < 3
                        && kotlin.math.abs(existing.bounds.h - adjusted.bounds.h) < 3
                }

                if (!isDuplicate) {
                    allElements.add(adjusted)
                }
            }
        }

        scrollable.scrollTo(0, 0)

        for (i in allElements.indices) {
            allElements[i] = allElements[i].copy(zIndex = i)
        }
        ViewTreeWalker.disambiguateIds(allElements)

        val scrollInfo = ScrollCaptureInfo(
            scrollView = scrollViewBounds,
            advancePx = scrollAmountPx,
            steps = steps,
            stepOffsets = stepOffsets,
        )

        return firstSchema.copy(elements = allElements, scrollCapture = scrollInfo)
    }

    private fun findScrollable(view: android.view.View): android.view.View? {
        val className = view.javaClass.name
        if (className.contains("RecyclerView") && view.canScrollVertically(1)) return view
        if (view is android.widget.ScrollView) return view
        if (className.contains("NestedScrollView") && view.canScrollVertically(1)) return view
        if (view is android.widget.HorizontalScrollView) return null
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                val found = findScrollable(view.getChildAt(i))
                if (found != null) return found
            }
        }
        return null
    }

    private fun boundsKey(e: SemanticElement): String {
        return "${e.bounds.x},${e.bounds.y},${e.bounds.w},${e.bounds.h}"
    }

    private fun handleOverlayOn(session: IHTTPSession): Response {
        val activity = currentActivity?.get()
            ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "no activity")

        val mode = session.parms?.get("mode") ?: "stroke"
        val latch = java.util.concurrent.CountDownLatch(1)
        var elemCount = 0

        mainHandler.post {
            try {
                overlayView?.let { (it.parent as? android.view.ViewGroup)?.removeView(it) }

                val schema = cachedSchema ?: ViewTreeWalker.walk(activity).also { cachedSchema = it }
                val elements = schema.elements.sortedBy { it.zIndex }
                elemCount = elements.size

                val decorView = activity.window.decorView as android.view.ViewGroup
                val density = activity.resources.displayMetrics.density
                val screenW = decorView.width
                val screenH = decorView.height

                val rects = elements.map { e ->
                    android.graphics.Rect(
                        (e.bounds.x * density).toInt(),
                        (e.bounds.y * density).toInt(),
                        ((e.bounds.x + e.bounds.w) * density).toInt(),
                        ((e.bounds.y + e.bounds.h) * density).toInt(),
                    )
                }
                val ids = elements.map { it.id }
                val boundsW = elements.map { (it.bounds.w * density).toInt() }
                val boundsH = elements.map { (it.bounds.h * density).toInt() }

                val overlay = object : android.view.View(activity) {
                    override fun onDraw(canvas: android.graphics.Canvas) {
                        super.onDraw(canvas)
                        canvas.drawColor(android.graphics.Color.WHITE)

                        for ((i, rect) in rects.withIndex()) {
                            if (mode == "fill") {
                                val isFullScreen = boundsW[i] >= (screenW * 0.95).toInt()
                                    && boundsH[i] >= (screenH * 0.95).toInt()
                                if (isFullScreen) continue
                            }

                            val hue = (djb2Hash(ids.getOrElse(i) { "" }).toUInt() % 360u).toFloat()
                            val elemColor = android.graphics.Color.HSVToColor(255, floatArrayOf(hue, 1f, 1f))

                            if (mode == "fill") {
                                val paint = android.graphics.Paint().apply {
                                    color = elemColor
                                    style = android.graphics.Paint.Style.FILL
                                }
                                canvas.drawRect(rect, paint)
                            } else {
                                val whitePaint = android.graphics.Paint().apply {
                                    color = android.graphics.Color.WHITE
                                    style = android.graphics.Paint.Style.FILL
                                }
                                canvas.drawRect(rect, whitePaint)

                                val sw = 4f
                                val paint = android.graphics.Paint().apply {
                                    color = elemColor
                                    style = android.graphics.Paint.Style.FILL
                                }
                                val l = rect.left.toFloat()
                                val t = rect.top.toFloat()
                                val r = rect.right.toFloat()
                                val b = rect.bottom.toFloat()
                                canvas.drawRect(l, t, r, t + sw, paint)
                                canvas.drawRect(l, b - sw, r, b, paint)
                                canvas.drawRect(l, t + sw, l + sw, b - sw, paint)
                                canvas.drawRect(r - sw, t + sw, r, b - sw, paint)
                            }
                        }
                    }
                }

                overlay.setBackgroundColor(android.graphics.Color.WHITE)
                decorView.addView(
                    overlay,
                    android.widget.FrameLayout.LayoutParams(
                        android.widget.FrameLayout.LayoutParams.MATCH_PARENT,
                        android.widget.FrameLayout.LayoutParams.MATCH_PARENT,
                    ),
                )
                overlayView = overlay
            } catch (_: Exception) {}
            latch.countDown()
        }

        latch.await(5, java.util.concurrent.TimeUnit.SECONDS)
        return jsonResponse("""{"overlay":"on","mode":"$mode","elements":$elemCount}""")
    }

    private fun handleOverlayOff(): Response {
        val latch = java.util.concurrent.CountDownLatch(1)
        mainHandler.post {
            overlayView?.let { (it.parent as? android.view.ViewGroup)?.removeView(it) }
            overlayView = null
            latch.countDown()
        }
        latch.await(2, java.util.concurrent.TimeUnit.SECONDS)
        return jsonResponse("""{"overlay":"off"}""")
    }

    private fun handleDebugLog(): Response {
        val entries = requestLog.map { e ->
            """{"ts":${e.ts},"method":"${e.method}","path":"${escape(e.path)}","status":${e.status},"duration_ms":${e.durationMs},"body_size":${e.bodySize}}"""
        }
        val walkLog = ViewTreeWalker.lastDebugLog.take(500)
        return jsonResponse("""{"requests":[${entries.joinToString(",")}],"last_walk":"${escape(walkLog)}"}""")
    }

    private fun handleQueryWhenIdle(session: IHTTPSession): Response {
        val contentLength = session.headers["content-length"]?.toIntOrNull() ?: 0
        val body = if (contentLength > 0) {
            val buf = ByteArray(contentLength)
            session.inputStream.read(buf, 0, contentLength)
            String(buf)
        } else "{}"

        val timeout = extractJsonInt(body, "timeout")?.toLong()?.times(1000) ?: 5000
        val resourceNames = extractJsonArray(body, "idle_resources")

        val idled = idleRegistry.waitForIdle(resourceNames, timeout)
        if (!idled) {
            val busy = idleRegistry.registeredNames().filter { name ->
                val r = idleRegistry.let { reg ->
                    try { !reg.isIdle(listOf(name)) } catch (_: Exception) { false }
                }
                r
            }
            return jsonResponse(
                """{"idle":false,"busy":${busy.map { "\"$it\"" }},"timeout_ms":$timeout}""",
                Response.Status.REQUEST_TIMEOUT,
            )
        }

        val match = extractJsonString(body, "match")
        return if (match != null) {
            handleSemantic()
        } else {
            jsonResponse("""{"idle":true}""")
        }
    }

    private fun handleIdleResources(): Response {
        val names = idleRegistry.registeredNames()
        val items = names.map { name ->
            val idle = try { idleRegistry.isIdle(listOf(name)) } catch (_: Exception) { false }
            """{"name":"$name","idle":$idle}"""
        }
        return jsonResponse("""{"resources":[${items.joinToString(",")}]}""")
    }

    private fun handleScrollSearch(session: IHTTPSession): Response {
        val contentLength = session.headers["content-length"]?.toIntOrNull() ?: 0
        val body = if (contentLength > 0) {
            val buf = ByteArray(contentLength)
            session.inputStream.read(buf, 0, contentLength)
            String(buf)
        } else "{}"

        val resourceNames = extractJsonArray(body, "idle_resources")
        val maxScroll = extractJsonInt(body, "max_scroll") ?: 10
        val restoreScroll = extractJsonBool(body, "restore_scroll") ?: false
        val matchObj = extractJsonString(body, "content_fuzzy")
            ?: extractNestedJsonString(body, "match", "content_fuzzy")
            ?: return jsonResponse("""{"found":false,"reason":"missing match.content_fuzzy"}""", Response.Status.BAD_REQUEST)

        idleRegistry.waitForIdle(resourceNames, 5000)

        val activity = currentActivity?.get()
            ?: return jsonResponse("""{"found":false,"scrolls":0,"reason":"no activity"}""")

        var found: SemanticElement? = null
        var scrollCount = 0
        var error: String? = null
        var scrollRestored = false
        val latch = java.util.concurrent.CountDownLatch(1)

        mainHandler.post {
            try {
                val target = matchObj.lowercase()
                val scrollable = findScrollable(activity.window.decorView)
                if (scrollable == null) {
                    error = "no scrollable view"
                    latch.countDown()
                    return@post
                }
                val density = activity.resources.displayMetrics.density
                val viewportH = (activity.resources.displayMetrics.heightPixels / density).toInt()
                val scrollAmountPx = (viewportH * 0.7 * density).toInt()

                val schema0 = ViewTreeWalker.walk(activity)
                found = schema0.elements.firstOrNull { it.content?.lowercase()?.contains(target) == true }

                if (found == null) {
                    for (step in 1..maxScroll) {
                        scrollable.scrollBy(0, scrollAmountPx)
                        Thread.sleep(300)
                        scrollCount = step
                        val schema = ViewTreeWalker.walk(activity)
                        found = schema.elements.firstOrNull { it.content?.lowercase()?.contains(target) == true }
                        if (found != null) break
                    }
                }

                if (restoreScroll || found == null) {
                    scrollable.scrollTo(0, 0)
                    scrollRestored = true
                }
            } catch (e: Exception) {
                error = e.message
            }
            latch.countDown()
        }

        if (!latch.await(30, java.util.concurrent.TimeUnit.SECONDS)) {
            return jsonResponse("""{"found":false,"scrolls":$scrollCount,"timeout":true}""", Response.Status.REQUEST_TIMEOUT)
        }
        if (error != null) {
            return jsonResponse("""{"found":false,"scrolls":$scrollCount,"reason":"${escape(error!!)}"}""")
        }

        return if (found != null) {
            val e = found!!
            jsonResponse("""{"found":true,"element":{"id":"${escape(e.id)}","content":"${escape(e.content ?: "")}","bounds":{"x":${e.bounds.x},"y":${e.bounds.y},"w":${e.bounds.w},"h":${e.bounds.h}},"clickable":${e.clickable},"tap_target":${e.tapTarget?.let { """{"x":${it.x},"y":${it.y},"w":${it.w},"h":${it.h}}""" } ?: "null"}},"scrolls":$scrollCount,"scroll_restored":$scrollRestored}""")
        } else {
            jsonResponse("""{"found":false,"scrolls":$scrollCount,"scroll_restored":$scrollRestored}""")
        }
    }

    private fun extractJsonBool(json: String, key: String): Boolean? {
        val pattern = """"$key"\s*:\s*(true|false)""".toRegex()
        return pattern.find(json)?.groupValues?.get(1)?.toBooleanStrictOrNull()
    }

    private fun extractNestedJsonString(json: String, parent: String, key: String): String? {
        val objPattern = """"$parent"\s*:\s*\{([^}]*)\}""".toRegex()
        val obj = objPattern.find(json)?.groupValues?.get(1) ?: return null
        return extractJsonString("{$obj}", key)
    }

    private fun extractJsonInt(json: String, key: String): Int? {
        val pattern = "\"$key\"\\s*:\\s*(\\d+)".toRegex()
        return pattern.find(json)?.groupValues?.get(1)?.toIntOrNull()
    }

    private fun extractJsonArray(json: String, key: String): List<String>? {
        val pattern = "\"$key\"\\s*:\\s*\\[([^\\]]*)\\]".toRegex()
        val match = pattern.find(json) ?: return null
        return match.groupValues[1].split(",")
            .map { it.trim().trim('"') }
            .filter { it.isNotEmpty() }
    }

    private fun toYaml(schema: SemanticSchema): String {
        val sb = StringBuilder()
        sb.appendLine("screen: \"${escape(schema.screen)}\"")
        sb.appendLine("device: \"${escape(schema.device)}\"")
        sb.appendLine("platform: ${schema.platform}")
        sb.appendLine("timestamp: \"${schema.timestamp}\"")
        sb.appendLine("viewport:")
        sb.appendLine("  width: ${schema.viewport.width}")
        sb.appendLine("  height: ${schema.viewport.height}")
        sb.appendLine("  density: ${schema.viewport.density}")
        schema.scrollCapture?.let { sc ->
            sb.appendLine("scroll_capture:")
            sb.appendLine("  scroll_view:")
            sb.appendLine("    x: ${sc.scrollView.x}")
            sb.appendLine("    y: ${sc.scrollView.y}")
            sb.appendLine("    w: ${sc.scrollView.w}")
            sb.appendLine("    h: ${sc.scrollView.h}")
            sb.appendLine("  advance_px: ${sc.advancePx}")
            sb.appendLine("  steps: ${sc.steps}")
            sb.appendLine("  step_offsets: [${sc.stepOffsets.joinToString(", ")}]")
        }
        sb.appendLine("elements:")

        for (e in schema.elements) {
            sb.appendLine("- id: \"${escape(e.id)}\"")
            e.platformId?.let { sb.appendLine("  platform_id: \"${escape(it)}\"") }
            sb.appendLine("  type: ${e.type}")
            e.content?.let { sb.appendLine("  content: \"${escape(it)}\"") }
            e.font?.let { f ->
                sb.appendLine("  font:")
                sb.appendLine("    family: ${f.family}")
                sb.appendLine("    weight: ${f.weight}")
                sb.appendLine("    size: ${f.size}")
            }
            e.color?.let { sb.appendLine("  foreground: \"${it}\"") }
            sb.appendLine("  bounds:")
            sb.appendLine("    x: ${e.bounds.x}")
            sb.appendLine("    y: ${e.bounds.y}")
            sb.appendLine("    w: ${e.bounds.w}")
            sb.appendLine("    h: ${e.bounds.h}")
            sb.appendLine("  z_index: ${e.zIndex}")
            sb.appendLine("  clickable: ${e.clickable}")
            sb.appendLine("  enabled: ${e.enabled}")
            sb.appendLine("  accessible: ${e.accessible}")
            e.a11yLabel?.let { sb.appendLine("  a11y_label: \"${escape(it)}\"") }
            e.a11yId?.let { sb.appendLine("  a11y_id: \"${escape(it)}\"") }
            e.background?.let { sb.appendLine("  background: \"${it}\"") }
            e.cornerRadius?.let { sb.appendLine("  corner_radius: $it") }
            e.padding?.let { p ->
                sb.appendLine("  padding:")
                sb.appendLine("    top: ${p.top}")
                sb.appendLine("    bottom: ${p.bottom}")
                sb.appendLine("    start: ${p.start}")
                sb.appendLine("    end: ${p.end}")
            }
            e.margin?.let { m ->
                sb.appendLine("  margin:")
                sb.appendLine("    top: ${m.top}")
                sb.appendLine("    bottom: ${m.bottom}")
                sb.appendLine("    start: ${m.start}")
                sb.appendLine("    end: ${m.end}")
            }
            e.lineCount?.let { sb.appendLine("  line_count: $it") }
            e.truncated?.let { sb.appendLine("  truncated: $it") }
            if (e.imageResource != null) {
                sb.appendLine("  image:")
                sb.appendLine("    resource: \"${e.imageResource}\"")
                sb.appendLine("    type: ${e.imageType ?: "raster"}")
            }
            e.imagePath?.let { sb.appendLine("  image_path: \"$it\"") }
            e.elevation?.let { sb.appendLine("  elevation: $it") }
            e.render?.let { sb.appendLine("  render: $it") }
            e.tapTarget?.let { t ->
                sb.appendLine("  tap_target:")
                sb.appendLine("    x: ${t.x}")
                sb.appendLine("    y: ${t.y}")
                sb.appendLine("    w: ${t.w}")
                sb.appendLine("    h: ${t.h}")
            }
        }

        return sb.toString()
    }

    private fun escape(s: String): String {
        return s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")
    }

    private fun djb2Hash(s: String): Int {
        var hash = 5381
        for (b in s.toByteArray()) {
            hash = hash * 33 + (b.toInt() and 0xFF)
        }
        return hash
    }

    companion object {
        private var instance: SemanticServer? = null

        @JvmStatic
        fun setContracts(navigator: AgentNavigator, auth: AgentAuth) {
            instance?.let {
                it.navigator = navigator
                it.auth = auth
            }
        }

        @JvmStatic
        @JvmOverloads
        fun install(
            app: Application,
            port: Int = 9876,
            gitHash: String = "",
            buildTime: String = "",
            navigator: AgentNavigator? = null,
            auth: AgentAuth? = null,
        ) {
            if (instance != null) return

            val server = SemanticServer(port, gitHash, buildTime)
            server.navigator = navigator
            server.auth = auth
            server.appRef = WeakReference(app)
            instance = server

            server.idleRegistry.register(UIThreadIdleResource())
            server.idleRegistry.register(LayoutIdleResource { server.currentActivity?.get() })
            server.idleRegistry.register(ScrollIdleResource { server.currentActivity?.get() })
            server.idleRegistry.register(NetworkIdleResource(app))
            server.idleRegistry.register(DialogIdleResource { server.currentActivity?.get() })
            val activityTransition = ActivityTransitionIdleResource()
            server.idleRegistry.register(activityTransition)

            app.registerActivityLifecycleCallbacks(activityTransition)
            app.registerActivityLifecycleCallbacks(object : Application.ActivityLifecycleCallbacks {
                override fun onActivityResumed(activity: Activity) {
                    server.currentActivity = WeakReference(activity)
                    server.cachedSchema = null
                    server.emitEvent("activity", """{"name":"${activity.javaClass.simpleName}","state":"resumed"}""")
                    activity.window.decorView.viewTreeObserver.addOnGlobalLayoutListener {
                        server.mainHandler.postDelayed({
                            val rootView = activity.window.decorView
                            val layoutIdle = !rootView.isLayoutRequested && !rootView.isDirty
                            val scrollIdle = server.isScrollIdle(rootView)
                            if (layoutIdle && scrollIdle) {
                                server.emitEvent("idle", """{"idle":true}""")
                            }
                        }, 100)
                    }
                }
                override fun onActivityPaused(activity: Activity) {
                    server.emitEvent("activity", """{"name":"${activity.javaClass.simpleName}","state":"paused"}""")
                }
                override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}
                override fun onActivityStarted(activity: Activity) {}
                override fun onActivityStopped(activity: Activity) {}
                override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
                override fun onActivityDestroyed(activity: Activity) {}
            })

            server.start()
            android.util.Log.i("SemanticAgent", "semantic server started on port $port")
        }
    }
}
