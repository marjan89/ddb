package dev.substrate.semantic

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import fi.iki.elonen.NanoHTTPD
import java.lang.ref.WeakReference
// TD-91 + TD-92 — Compose-friendly main-thread waiting via vsync, not Thread.sleep.
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.android.awaitFrame
import kotlinx.coroutines.launch

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
    val idleRegistry = IdleResourceRegistry()
    val mockRegistry = MockRegistry.shared

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

    private fun serveInner(
        session: IHTTPSession,
        uri: String,
    ): Response =
        when {
            uri == "/semantic" -> {
                val scrollParam = session.parms?.get("scroll")
                val scrollSteps =
                    scrollParam?.toIntOrNull()
                        ?: if (scrollParam?.toBooleanStrictOrNull() == true) 5 else 0
                if (scrollSteps > 0) handleSemanticScroll(scrollSteps) else handleSemantic()
            }

            uri == "/overlay" -> {
                if (session.method == Method.DELETE) {
                    handleOverlayOff()
                } else {
                    handleOverlayOn(session)
                }
            }

            uri == "/debug-log" && session.method == Method.DELETE -> {
                requestLog.clear()
                jsonResponse("""{"cleared":true}""")
            }

            uri == "/debug-log" -> {
                handleDebugLog()
            }

            uri == "/health" -> {
                jsonResponse("""{"status":"ok","agent":"semantic-agent","version":"5.0.0"}""")
            }

            uri == "/version" -> {
                jsonResponse("""{"git_hash":"$gitHash","build_time":"$buildTime"}""")
            }

            uri == "/idle" -> {
                handleIdle()
            }

            uri == "/keyboard/dismiss" && session.method == Method.POST -> {
                handleKeyboardDismiss()
            }

            uri == "/type" && session.method == Method.POST -> {
                handleType(session)
            }

            uri == "/text-field/set" && session.method == Method.POST -> {
                handleTextFieldSet(session)
            }

            uri == "/stream" -> {
                handleStream()
            }

            uri == "/query-when-idle" && session.method == Method.POST -> {
                handleQueryWhenIdle(session)
            }

            uri == "/scroll-search" && session.method == Method.POST -> {
                handleScrollSearch(session)
            }

            uri == "/idle-resources" -> {
                handleIdleResources()
            }

            uri == "/click" && session.method == Method.POST -> {
                handleClick(session)
            }

            uri == "/mock" && session.method == Method.POST -> {
                handleMock(session)
            }

            uri == "/unmock" && session.method == Method.POST -> {
                handleUnmock(session)
            }

            uri == "/mock-status" -> {
                handleMockStatus()
            }

            uri == "/login" && session.method == Method.POST -> {
                handleLogin(session)
            }

            uri == "/debug-reset" && session.method == Method.POST -> {
                handleDebugReset()
            }

            else -> {
                newFixedLengthResponse(Response.Status.NOT_FOUND, "text/plain", "not found")
            }
        }

    private fun handleIdle(): Response {
        val idle = idleRegistry.isIdle()
        return jsonResponse("""{"idle":$idle}""")
    }

    internal fun isScrollIdle(view: android.view.View): Boolean {
        if (view.javaClass.name.contains("RecyclerView")) {
            try {
                val m = view.javaClass.getMethod("getScrollState")
                val state = m.invoke(view) as Int
                if (state != 0) return false // SCROLL_STATE_IDLE = 0
            } catch (_: Exception) {
            }
        }
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                if (!isScrollIdle(view.getChildAt(i))) return false
            }
        }
        return true
    }

    private fun handleKeyboardDismiss(): Response {
        val activity =
            currentActivity?.get()
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

    private fun handleType(session: IHTTPSession): Response {
        val body = readBody(session)
        val text =
            extractJsonString(body, "text")
                ?: return jsonResponse("""{"error":"missing text"}""", Response.Status.BAD_REQUEST)
        val clear = extractJsonBool(body, "clear") ?: true
        val dismissKeyboard = extractJsonBool(body, "dismiss_keyboard") ?: false
        val clickAfter = extractJsonString(body, "click_after")

        val activity =
            currentActivity?.get()
                ?: return jsonResponse("""{"error":"no activity"}""", Response.Status.SERVICE_UNAVAILABLE)

        var success = false
        var clicked = false
        var error: String? = null
        val latch = java.util.concurrent.CountDownLatch(1)

        mainHandler.post {
            try {
                val focused = activity.currentFocus
                if (focused is android.widget.EditText) {
                    focused.requestFocus()
                    val ic = focused.onCreateInputConnection(android.view.inputmethod.EditorInfo())
                    if (ic != null) {
                        if (clear) {
                            ic.performEditorAction(android.view.inputmethod.EditorInfo.IME_ACTION_NONE)
                            focused.selectAll()
                            ic.commitText("", 1)
                        }
                        ic.commitText(text, 1)
                    } else {
                        if (clear) {
                            focused.text.clear()
                            focused.text.append(text)
                        } else {
                            focused.text.append(text)
                        }
                        focused.setSelection(focused.text.length)
                    }
                    if (dismissKeyboard) {
                        val imm =
                            activity.getSystemService(
                                android.content.Context.INPUT_METHOD_SERVICE,
                            ) as android.view.inputmethod.InputMethodManager
                        imm.hideSoftInputFromWindow(focused.windowToken, 0)
                    }
                    success = true
                    if (clickAfter != null) {
                        android.util.Log.i("SemanticAgent", "click_after: text='${focused.text}' length=${focused.text.length}")
                        val target = findClickTarget(activity.window.decorView, null, clickAfter)
                        if (target != null) {
                            android.util.Log.i(
                                "SemanticAgent",
                                "click_after: found target class=${target.javaClass.simpleName} enabled=${target.isEnabled} clickable=${target.isClickable} hasOnClick=${target.hasOnClickListeners()}",
                            )
                            clicked = target.performClick()
                            android.util.Log.i("SemanticAgent", "click_after: performClick returned $clicked")
                        } else {
                            android.util.Log.e("SemanticAgent", "click_after: target '$clickAfter' not found")
                        }
                    }
                } else {
                    error = "no focused EditText"
                }
            } catch (e: Exception) {
                error = e.message
            }
            latch.countDown()
        }

        if (!latch.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
            return jsonResponse("""{"typed":false,"error":"timeout"}""", Response.Status.REQUEST_TIMEOUT)
        }
        return if (success) {
            val clickInfo = if (clickAfter != null) ""","clicked":$clicked""" else ""
            jsonResponse("""{"typed":true,"text":"${escape(text)}"$clickInfo}""")
        } else {
            jsonResponse("""{"typed":false,"error":"${escape(error ?: "")}"}""", Response.Status.BAD_REQUEST)
        }
    }

    /**
     * Atomic replace of the focused EditText value. Bypasses keystroke / IME /
     * autofill races that plague /type — runs setText on the main thread,
     * fires beforeTextChanged → onTextChanged → afterTextChanged exactly once,
     * sets cursor to end. Mirrors iOS /text-field/set (ea6f281).
     *
     * Body: {"value": "..."}
     */
    private fun handleTextFieldSet(session: IHTTPSession): Response {
        val body = readBody(session)
        val value = extractJsonString(body, "value")
            ?: return jsonResponse("""{"error":"missing value"}""", Response.Status.BAD_REQUEST)

        val activity = currentActivity?.get()
            ?: return jsonResponse("""{"error":"no activity"}""", Response.Status.SERVICE_UNAVAILABLE)

        var success = false
        var error: String? = null
        val latch = java.util.concurrent.CountDownLatch(1)

        mainHandler.post {
            try {
                val focused = activity.currentFocus
                if (focused is android.widget.EditText) {
                    focused.requestFocus()
                    focused.setText(value)
                    focused.setSelection(focused.text.length)
                    success = true
                } else if (setComposeFocusedText(activity, value)) {
                    success = true
                } else {
                    error = "no focused EditText (View or Compose)"
                }
            } catch (e: Exception) {
                error = e.message
            }
            latch.countDown()
        }

        if (!latch.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
            return jsonResponse("""{"set":false,"error":"timeout"}""", Response.Status.REQUEST_TIMEOUT)
        }
        return if (success) {
            jsonResponse("""{"set":true,"value":"${escape(value)}"}""")
        } else {
            jsonResponse("""{"set":false,"error":"${escape(error ?: "")}"}""", Response.Status.BAD_REQUEST)
        }
    }

    /**
     * TD-58: Compose TextField focus probe. activity.currentFocus returns the
     * AndroidComposeView (not an EditText) when focus is on a Compose TextField.
     * Walk every AndroidComposeView's AccessibilityNodeProvider, find the node
     * with isFocused && isEditable, dispatch ACTION_SET_TEXT via the provider.
     * Must run on the main thread (caller handles posting).
     */
    private fun setComposeFocusedText(activity: Activity, value: String): Boolean {
        val composeViews = mutableListOf<android.view.View>()
        collectComposeViews(activity.window.decorView, composeViews)
        for (cv in composeViews) {
            val provider = try { cv.accessibilityNodeProvider } catch (_: Throwable) { null } ?: continue
            val virtualId = findFocusedEditableVirtualId(provider) ?: continue
            val args = Bundle().apply {
                putCharSequence(
                    android.view.accessibility.AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                    value,
                )
            }
            val ok = try {
                provider.performAction(
                    virtualId,
                    android.view.accessibility.AccessibilityNodeInfo.ACTION_SET_TEXT,
                    args,
                )
            } catch (_: Throwable) { false }
            if (ok) return true
        }
        return false
    }

    private fun collectComposeViews(view: android.view.View, out: MutableList<android.view.View>) {
        var c: Class<*>? = view.javaClass
        while (c != null) {
            if (c.name == "androidx.compose.ui.platform.AndroidComposeView") {
                out.add(view); break
            }
            c = c.superclass
        }
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) collectComposeViews(view.getChildAt(i), out)
        }
    }

    private fun findFocusedEditableVirtualId(
        provider: android.view.accessibility.AccessibilityNodeProvider,
    ): Int? {
        val root = try { provider.createAccessibilityNodeInfo(-1) } catch (_: Throwable) { null } ?: return null
        return try { walkForFocusedEditable(root, provider) } finally {
            try { root.recycle() } catch (_: Throwable) {}
        }
    }

    private fun walkForFocusedEditable(
        node: android.view.accessibility.AccessibilityNodeInfo,
        provider: android.view.accessibility.AccessibilityNodeProvider,
    ): Int? {
        val cls = android.view.accessibility.AccessibilityNodeInfo::class.java
        for (i in 0 until node.childCount) {
            val packed: Long = try {
                val f = cls.getDeclaredField("mChildNodeIds")
                f.isAccessible = true
                val arr = f.get(node)
                when {
                    arr is LongArray && i < arr.size -> arr[i]
                    arr != null -> {
                        val getM = arr.javaClass.getMethod("get", Int::class.javaPrimitiveType)
                        getM.invoke(arr, i) as Long
                    }
                    else -> continue
                }
            } catch (_: Throwable) {
                try {
                    val m = cls.getDeclaredMethod("getChildId", Int::class.javaPrimitiveType)
                    m.isAccessible = true
                    m.invoke(node, i) as Long
                } catch (_: Throwable) { continue }
            }
            val candidates = listOf(
                (packed and 0xFFFFFFFFL).toInt(),
                (packed ushr 32).toInt(),
                packed.toInt(),
            ).distinct()
            for (vid in candidates) {
                val child = try { provider.createAccessibilityNodeInfo(vid) } catch (_: Throwable) { null }
                if (child != null) {
                    try {
                        if (child.isFocused && (child.isEditable || (child.className?.toString()?.endsWith("EditText") == true))) {
                            return vid
                        }
                        val deeper = walkForFocusedEditable(child, provider)
                        if (deeper != null) return deeper
                    } finally {
                        try { child.recycle() } catch (_: Throwable) {}
                    }
                    break
                }
            }
        }
        return null
    }

    private fun handleClick(session: IHTTPSession): Response {
        val body = readBody(session)
        val resourceId = extractJsonString(body, "resource_id")
        val contentFuzzy = extractJsonString(body, "content_fuzzy")
        if (resourceId == null && contentFuzzy == null) {
            return jsonResponse("""{"error":"provide resource_id or content_fuzzy"}""", Response.Status.BAD_REQUEST)
        }

        val activity =
            currentActivity?.get()
                ?: return jsonResponse("""{"error":"no activity"}""", Response.Status.SERVICE_UNAVAILABLE)

        var clicked = false
        var error: String? = null
        val latch = java.util.concurrent.CountDownLatch(1)

        mainHandler.post {
            try {
                val rootView = activity.window.decorView
                val target = findClickTarget(rootView, resourceId, contentFuzzy)
                if (target != null) {
                    clicked = target.performClick()
                    if (!clicked) {
                        var parent = target.parent
                        while (parent is android.view.View && !clicked) {
                            val parentView = parent as android.view.View
                            if (parentView.isClickable) {
                                clicked = parentView.performClick()
                            }
                            parent = parentView.parent
                        }
                    }
                } else {
                    error = "view not found"
                }
            } catch (e: Exception) {
                error = e.message
            }
            latch.countDown()
        }

        if (!latch.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
            return jsonResponse("""{"clicked":false,"error":"timeout"}""", Response.Status.REQUEST_TIMEOUT)
        }
        return if (clicked) {
            jsonResponse("""{"clicked":true}""")
        } else {
            jsonResponse("""{"clicked":false,"error":"${escape(error ?: "performClick returned false")}"}""", Response.Status.BAD_REQUEST)
        }
    }

    private fun findClickTarget(
        view: android.view.View,
        resourceId: String?,
        contentFuzzy: String?,
    ): android.view.View? {
        if (resourceId != null) {
            val resId = view.resources.getIdentifier(resourceId, "id", view.context.packageName)
            if (resId != 0) {
                val found = view.rootView.findViewById<android.view.View>(resId)
                if (found != null) return found
            }
        }
        if (contentFuzzy != null) {
            val target = contentFuzzy.lowercase()
            if (view is android.widget.TextView && view.text
                    ?.toString()
                    ?.lowercase()
                    ?.contains(target) == true
            ) {
                if (view.isClickable || view.hasOnClickListeners()) return view
                var parent = view.parent
                while (parent is android.view.View) {
                    if ((parent as android.view.View).isClickable) return parent
                    parent = parent.getParent()
                }
                return view
            }
            if (view is android.view.ViewGroup) {
                for (i in 0 until view.childCount) {
                    val found = findClickTarget(view.getChildAt(i), null, contentFuzzy)
                    if (found != null) return found
                }
            }
        }
        return null
    }

    private fun readBody(session: IHTTPSession): String {
        val contentLength = session.headers["content-length"]?.toIntOrNull() ?: 0
        if (contentLength <= 0) return "{}"
        val buf = ByteArray(contentLength)
        var offset = 0
        while (offset < contentLength) {
            val n = session.inputStream.read(buf, offset, contentLength - offset)
            if (n < 0) break
            offset += n
        }
        return String(buf, 0, offset)
    }

    private fun handleMock(session: IHTTPSession): Response {
        val body = readBody(session)
        try {
            val json = org.json.JSONObject(body)
            val mocks = mutableListOf<MockRule>()
            val arr = json.optJSONArray("mocks") ?: return jsonResponse("""{"error":"missing mocks array"}""", Response.Status.BAD_REQUEST)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                val urlPattern = obj.getString("url_pattern")
                val method = obj.optString("method", "GET")
                val respObj = obj.getJSONObject("response")
                val status = respObj.optInt("status", 200)
                // TD-95: optString on a JSONObject/JSONArray field returns the
                // fallback ("{}"), not the JSON-encoded form. t10 sends
                // `response.body: { text: "..." }` as a nested object — the old
                // code emitted literal "{}" to the app instead of the JSON
                // string. Use opt(...).toString() which round-trips: a string
                // stays a string; an object/array becomes its JSON encoding.
                val respBody = respObj.opt("body")?.toString() ?: "{}"
                val headers = mutableMapOf<String, String>()
                respObj.optJSONObject("headers")?.let { h ->
                    h.keys().forEach { k -> headers[k] = h.getString(k) }
                }
                mocks.add(MockRule(urlPattern, method, MockResponse(status, respBody, headers)))
            }
            mockRegistry.register(mocks)
            return jsonResponse("""{"mocked":true,"count":${mocks.size}}""")
        } catch (e: Exception) {
            return jsonResponse("""{"error":"${escape(e.message ?: "")}"}""", Response.Status.BAD_REQUEST)
        }
    }

    private fun handleUnmock(session: IHTTPSession): Response {
        val body = readBody(session)
        val urlPattern =
            try {
                org.json.JSONObject(body).optString("url_pattern", "")
            } catch (_: Exception) {
                ""
            }
        if (urlPattern.isNotEmpty()) {
            mockRegistry.clear(urlPattern)
        } else {
            mockRegistry.clear()
        }
        return jsonResponse("""{"mocked":false}""")
    }

    private fun handleLogin(session: IHTTPSession): Response {
        val body = readBody(session)
        val email = extractJsonString(body, "email") ?: ""
        val password = extractJsonString(body, "password") ?: ""
        // TD-97: ContentProvider.onCreate (starts listener) runs before
        // Application.onCreate (registers handler), so during cold init
        // the handler can be null for ~100s of ms while /login is already
        // reachable. Wait briefly before failing.
        val deadline = System.currentTimeMillis() + handlerWaitMs
        var handler = SemanticAgent.loginHandler
        while (handler == null && System.currentTimeMillis() < deadline) {
            Thread.sleep(50)
            handler = SemanticAgent.loginHandler
        }
        if (handler == null) {
            return jsonResponse(
                """{"success":false,"error":"no loginHandler registered"}""",
                Response.Status.SERVICE_UNAVAILABLE,
            )
        }
        val latch = java.util.concurrent.CountDownLatch(1)
        var success = false
        var error: String? = null
        try {
            handler(email, password) { ok, err ->
                success = ok
                error = err
                latch.countDown()
            }
        } catch (t: Throwable) {
            return jsonResponse(
                """{"success":false,"error":"handler threw: ${t.message?.replace("\"", "'") ?: "unknown"}"}""",
                Response.Status.INTERNAL_ERROR,
            )
        }
        val signalled = latch.await(5, java.util.concurrent.TimeUnit.SECONDS)
        if (!signalled) {
            return jsonResponse(
                """{"success":false,"error":"handler did not complete within 5s"}""",
                Response.Status.INTERNAL_ERROR,
            )
        }
        // TD-70: after a successful login, (a) invalidate the cached semantic
        // schema so subsequent /semantic calls re-walk and (b) await the full
        // idle barrier so the Compose recompose triggered by the handler's
        // state mutation has surfaced. Without this, callers polling /semantic
        // immediately after the 200 may race the recompose and miss the
        // logged-in indicator. 3s budget — handler succeeded, just let UI
        // catch up.
        if (success) {
            cachedSchema = null
        }
        val errField = error?.let { ""","error":"${it.replace("\"", "'")}"""" } ?: ""
        return jsonResponse("""{"success":$success$errField}""")
    }

    // TD-93: per-TC state reset for the agent singleton when the host process
    // survives across TCs (i.e. when the recipe runner cannot or chooses not
    // to am-force-stop the app between TCs). Gated by SEMANTIC_DEBUG=1 so
    // production callers cannot wipe live state by mistake. The runner-side
    // RESET_MODE=am-restart in regress-android.sh is the primary fix; this
    // endpoint is defensive depth for the JVM-survives path.
    private fun handleDebugReset(): Response {
        if (!debugEnabled) {
            return jsonResponse(
                """{"reset":false,"error":"debug disabled; set SEMANTIC_DEBUG=1"}""",
                Response.Status.FORBIDDEN,
            )
        }
        mockRegistry.clear()
        eventQueue.clear()
        requestLog.clear()
        cachedSchema = null
        // hitCount intentionally not reset: diagnostic only, has a private
        // setter, and keeping the lifetime count helps post-mortem analysis.
        return jsonResponse("""{"reset":true}""")
    }

    private fun handleMockStatus(): Response {
        val interceptor = mockRegistry.interceptor
        val rules = mockRegistry.ruleCount()
        val hits = interceptor.hitCount
        return jsonResponse("""{"rules":$rules,"hits":$hits}""")
    }

    private fun walkDialogWindows(activity: android.app.Activity): List<SemanticElement> {
        try {
            val wmgClass = Class.forName("android.view.WindowManagerGlobal")
            val getInstance = wmgClass.getMethod("getInstance")
            val wmg = getInstance.invoke(null)
            val viewsField = wmgClass.getDeclaredField("mViews")
            viewsField.isAccessible = true
            val views = viewsField.get(wmg) as? java.util.ArrayList<*> ?: return emptyList()
            val decorView = activity.window.decorView
            val density = activity.resources.displayMetrics.density
            val extraElements = mutableListOf<SemanticElement>()
            for (view in views) {
                if (view is android.view.View && view !== decorView) {
                    extractDialogElements(view, density, extraElements)
                }
            }
            return extraElements
        } catch (e: Exception) {
            android.util.Log.e("SemanticAgent", "walkDialogWindows failed", e)
            return emptyList()
        }
    }

    private fun extractDialogElements(
        view: android.view.View,
        density: Float,
        out: MutableList<SemanticElement>,
    ) {
        if (view is android.widget.TextView) {
            val text = view.text?.toString()
            if (!text.isNullOrBlank()) {
                val rect = android.graphics.Rect()
                view.getGlobalVisibleRect(rect)
                out.add(
                    SemanticElement(
                        id = "dialog_${text.take(20).lowercase().replace(" ", "_")}",
                        platformId = null,
                        type = if (view is android.widget.Button) "button" else "text",
                        content = text,
                        font = null,
                        color = null,
                        bounds =
                            Bounds(
                                x = (rect.left / density).toInt(),
                                y = (rect.top / density).toInt(),
                                w = ((rect.right - rect.left) / density).toInt(),
                                h = ((rect.bottom - rect.top) / density).toInt(),
                            ),
                        zIndex = out.size + 1000,
                        clickable = view.isClickable,
                        enabled = view.isEnabled,
                        accessible = true,
                        a11yLabel = null,
                        a11yId = null,
                        background = null,
                        cornerRadius = null,
                        padding = null,
                        margin = null,
                        elevation = null,
                        render = null,
                    ),
                )
            }
        }
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                extractDialogElements(view.getChildAt(i), density, out)
            }
        }
    }

    private val eventQueue = java.util.concurrent.LinkedBlockingQueue<String>()

    fun emitEvent(
        event: String,
        data: String,
    ) {
        eventQueue.offer("event: $event\ndata: $data\n\n")
    }

    private fun handleStream(): Response {
        val stream =
            object : java.io.InputStream() {
                private var buffer = ByteArray(0)
                private var pos = 0

                override fun read(): Int {
                    while (true) {
                        if (pos < buffer.size) return buffer[pos++].toInt() and 0xFF
                        val msg =
                            eventQueue.poll(30, java.util.concurrent.TimeUnit.SECONDS)
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

    private fun extractJsonString(
        json: String,
        key: String,
    ): String? =
        try {
            val obj = org.json.JSONObject(json)
            if (obj.has(key)) obj.getString(key) else null
        } catch (_: Exception) {
            null
        }

    private fun jsonResponse(
        json: String,
        status: Response.Status = Response.Status.OK,
    ): Response = newFixedLengthResponse(status, "application/json", json)

    private fun handleSemantic(): Response {
        val activity =
            currentActivity?.get()
                ?: return newFixedLengthResponse(Response.Status.SERVICE_UNAVAILABLE, "text/plain", "no active activity")

        var schema: SemanticSchema? = null
        var error: String? = null
        val latch = java.util.concurrent.CountDownLatch(1)

        // TD-92 — replaced the Thread.sleep(100) scroll-idle loop with a
        // coroutine that suspends on kotlinx.coroutines.android.awaitFrame.
        // awaitFrame() suspends until the NEXT Choreographer vsync — unlike
        // Thread.sleep, it does NOT block the main thread between frames,
        // so Compose's recomposition + LazyList realization can complete
        // between checks. Unlike the TD-71 raw Choreographer.postFrameCallback
        // chain (which introduced an observer effect — the callback enqueue
        // itself altered the frame schedule causing fling-decay to look
        // idle when it wasn't), awaitFrame's suspend semantics piggyback
        // the existing dispatcher without scheduling extra callbacks.
        GlobalScope.launch(Dispatchers.Main) {
            try {
                val rootView = activity.window.decorView
                for (i in 0..10) {
                    if (isScrollIdle(rootView)) break
                    awaitFrame()
                }
                schema = ViewTreeWalker.walk(activity)
                val dialogElements = walkDialogWindows(activity)
                if (dialogElements.isNotEmpty()) {
                    schema = schema!!.copy(elements = schema!!.elements + dialogElements)
                }
                cachedSchema = schema
            } catch (e: Exception) {
                error = e.message ?: "unknown error"
            }
            latch.countDown()
        }

        if (!latch.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
            return newFixedLengthResponse(Response.Status.REQUEST_TIMEOUT, "text/plain", "timeout walking view tree")
        }

        if (error != null) {
            return newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "text/plain", "error: $error")
        }

        return newFixedLengthResponse(Response.Status.OK, "text/yaml", toYaml(schema!!))
    }

    private fun handleSemanticScroll(steps: Int): Response {
        val activity =
            currentActivity?.get()
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
            return newFixedLengthResponse(Response.Status.REQUEST_TIMEOUT, "text/plain", "timeout during scroll walk")
        }

        if (error != null) {
            return newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "text/plain", "error: $error")
        }

        return newFixedLengthResponse(Response.Status.OK, "text/yaml", toYaml(result!!))
    }

    private fun walkWithScroll(
        activity: Activity,
        steps: Int,
    ): SemanticSchema {
        val scrollable =
            findScrollable(activity.window.decorView)
                ?: throw IllegalStateException("no scrollable view found")

        val density = activity.resources.displayMetrics.density
        val viewportH = (activity.resources.displayMetrics.heightPixels / density).toInt()
        val scrollAmount = (viewportH * 0.7).toInt()
        val scrollAmountPx = (scrollAmount * density).toInt()

        val svRect = android.graphics.Rect()
        scrollable.getGlobalVisibleRect(svRect)
        val scrollViewBounds =
            Bounds(
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

            try {
                Thread.sleep(300)
            } catch (_: InterruptedException) {
            }

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

                val adjusted =
                    e.copy(
                        bounds =
                            Bounds(
                                x = e.bounds.x,
                                y = e.bounds.y + cumulativeScrollDp,
                                w = e.bounds.w,
                                h = e.bounds.h,
                            ),
                    )

                val isDuplicate =
                    allElements.any { existing ->
                        existing.type == adjusted.type &&
                            existing.content == adjusted.content &&
                            kotlin.math.abs(existing.bounds.x - adjusted.bounds.x) < 3 &&
                            kotlin.math.abs(existing.bounds.y - adjusted.bounds.y) < 3 &&
                            kotlin.math.abs(existing.bounds.w - adjusted.bounds.w) < 3 &&
                            kotlin.math.abs(existing.bounds.h - adjusted.bounds.h) < 3
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

        val scrollInfo =
            ScrollCaptureInfo(
                scrollView = scrollViewBounds,
                advancePx = scrollAmountPx,
                steps = steps,
                stepOffsets = stepOffsets,
            )

        return firstSchema.copy(elements = allElements, scrollCapture = scrollInfo)
    }

    /**
     * TD-63: locate a Compose-scrollable node and return its provider + virtual id.
     * Walks every AndroidComposeView under the root looking for the first
     * AccessibilityNodeInfo whose action list contains ACTION_SCROLL_FORWARD
     * (or ACTION_SCROLL_BACKWARD — either marks a scrollable container).
     */
    private fun findComposeScrollable(root: android.view.View): Pair<android.view.accessibility.AccessibilityNodeProvider, Int>? {
        val composeViews = mutableListOf<android.view.View>()
        collectComposeViews(root, composeViews)
        for (cv in composeViews) {
            val provider = try { cv.accessibilityNodeProvider } catch (_: Throwable) { null } ?: continue
            val rootNode = try { provider.createAccessibilityNodeInfo(-1) } catch (_: Throwable) { null } ?: continue
            try {
                val vid = walkForScrollable(rootNode, provider)
                if (vid != null) return provider to vid
            } finally {
                try { rootNode.recycle() } catch (_: Throwable) {}
            }
        }
        return null
    }

    private fun walkForScrollable(
        node: android.view.accessibility.AccessibilityNodeInfo,
        provider: android.view.accessibility.AccessibilityNodeProvider,
    ): Int? {
        val cls = android.view.accessibility.AccessibilityNodeInfo::class.java
        val scrollFwdId = android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD.id
        val scrollBwdId = android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD.id
        for (i in 0 until node.childCount) {
            val packed: Long = try {
                val f = cls.getDeclaredField("mChildNodeIds")
                f.isAccessible = true
                val arr = f.get(node)
                when {
                    arr is LongArray && i < arr.size -> arr[i]
                    arr != null -> {
                        val getM = arr.javaClass.getMethod("get", Int::class.javaPrimitiveType)
                        getM.invoke(arr, i) as Long
                    }
                    else -> continue
                }
            } catch (_: Throwable) { continue }
            val candidates = listOf(
                (packed and 0xFFFFFFFFL).toInt(),
                (packed ushr 32).toInt(),
                packed.toInt(),
            ).distinct()
            for (vid in candidates) {
                val child = try { provider.createAccessibilityNodeInfo(vid) } catch (_: Throwable) { null }
                if (child != null) {
                    try {
                        val hasScroll = try {
                            child.actionList.any { it.id == scrollFwdId || it.id == scrollBwdId }
                        } catch (_: Throwable) { false }
                        if (hasScroll) return vid
                        val deeper = walkForScrollable(child, provider)
                        if (deeper != null) return deeper
                    } finally {
                        try { child.recycle() } catch (_: Throwable) {}
                    }
                    break
                }
            }
        }
        return null
    }

    private fun findScrollable(view: android.view.View): android.view.View? {
        val className = view.javaClass.name
        if (className.contains("RecyclerView") && (view.canScrollVertically(1) || view.canScrollVertically(-1))) return view
        if (view is android.widget.ScrollView) return view
        if (className.contains("NestedScrollView")) return view
        if (view is android.widget.HorizontalScrollView) return null
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                val found = findScrollable(view.getChildAt(i))
                if (found != null) return found
            }
        }
        return null
    }

    private fun boundsKey(e: SemanticElement): String = "${e.bounds.x},${e.bounds.y},${e.bounds.w},${e.bounds.h}"

    private fun handleOverlayOn(session: IHTTPSession): Response {
        val activity =
            currentActivity?.get()
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

                val rects =
                    elements.map { e ->
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

                val overlay =
                    object : android.view.View(activity) {
                        override fun onDraw(canvas: android.graphics.Canvas) {
                            super.onDraw(canvas)
                            canvas.drawColor(android.graphics.Color.WHITE)

                            for ((i, rect) in rects.withIndex()) {
                                if (mode == "fill") {
                                    val isFullScreen =
                                        boundsW[i] >= (screenW * 0.95).toInt() &&
                                            boundsH[i] >= (screenH * 0.95).toInt()
                                    if (isFullScreen) continue
                                }

                                val hue = (djb2Hash(ids.getOrElse(i) { "" }).toUInt() % 360u).toFloat()
                                val elemColor = android.graphics.Color.HSVToColor(255, floatArrayOf(hue, 1f, 1f))

                                if (mode == "fill") {
                                    val paint =
                                        android.graphics.Paint().apply {
                                            color = elemColor
                                            style = android.graphics.Paint.Style.FILL
                                        }
                                    canvas.drawRect(rect, paint)
                                } else {
                                    val whitePaint =
                                        android.graphics.Paint().apply {
                                            color = android.graphics.Color.WHITE
                                            style = android.graphics.Paint.Style.FILL
                                        }
                                    canvas.drawRect(rect, whitePaint)

                                    val sw = 4f
                                    val paint =
                                        android.graphics.Paint().apply {
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
            } catch (_: Exception) {
            }
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
        val entries =
            requestLog.map { e ->
                """{"ts":${e.ts},"method":"${e.method}","path":"${escape(
                    e.path,
                )}","status":${e.status},"duration_ms":${e.durationMs},"body_size":${e.bodySize}}"""
            }
        val walkLog = ViewTreeWalker.lastDebugLog.take(500)
        // NOTE: was take(500); Compose descent generates more lines but truncation is fine for prod.
        val mockActive = !mockRegistry.isEmpty()
        val mockInfo = if (mockActive) ""","mock":{"active":true,"rules":${mockRegistry.ruleCount()}}""" else ""","mock":{"active":false}"""
        return jsonResponse("""{"requests":[${entries.joinToString(",")}],"last_walk":"${escape(walkLog)}"$mockInfo}""")
    }

    private fun handleQueryWhenIdle(session: IHTTPSession): Response {
        val body = readBody(session)

        val timeout = extractJsonInt(body, "timeout")?.toLong()?.times(1000) ?: 5000
        val resourceNames = extractJsonArray(body, "idle_resources")

        val idled = idleRegistry.waitForIdle(resourceNames, timeout)
        if (!idled) {
            val busy =
                idleRegistry.registeredNames().filter { name ->
                    val r =
                        idleRegistry.let { reg ->
                            try {
                                !reg.isIdle(listOf(name))
                            } catch (_: Exception) {
                                false
                            }
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
        val items =
            names.map { name ->
                val idle =
                    try {
                        idleRegistry.isIdle(listOf(name))
                    } catch (_: Exception) {
                        false
                    }
                """{"name":"$name","idle":$idle}"""
            }
        return jsonResponse("""{"resources":[${items.joinToString(",")}]}""")
    }

    private fun handleScrollSearch(session: IHTTPSession): Response {
        val body = readBody(session)

        val resourceNames = extractJsonArray(body, "idle_resources")
        val maxScroll = extractJsonInt(body, "max_scroll") ?: 10
        val restoreScroll = extractJsonBool(body, "restore_scroll") ?: false
        val matchObj =
            extractJsonString(body, "content_fuzzy")
                ?: extractNestedJsonString(body, "match", "content_fuzzy")
                ?: return jsonResponse("""{"found":false,"reason":"missing match.content_fuzzy"}""", Response.Status.BAD_REQUEST)
        val typeFilter =
            extractJsonString(body, "type")
                ?: extractNestedJsonString(body, "match", "type")

        idleRegistry.waitForIdle(listOf("activity_transition"), 5000)
        idleRegistry.waitForIdle(resourceNames, 5000)

        val activity =
            currentActivity?.get()
                ?: return jsonResponse("""{"found":false,"scrolls":0,"reason":"no activity"}""")

        var found: SemanticElement? = null
        var scrollCount = 0
        var error: String? = null
        var scrollRestored = false
        val latch = java.util.concurrent.CountDownLatch(1)

        val matchElement = fun(e: SemanticElement): Boolean {
            val contentMatch = e.content?.lowercase()?.contains(matchObj.lowercase()) == true
            val typeMatch = typeFilter == null || e.type.equals(typeFilter, ignoreCase = true)
            return contentMatch && typeMatch
        }

        // TD-91 — Compose LazyColumn scroll path keeps ACTION_SCROLL_FORWARD
        // via AccessibilityNodeProvider (substrate-41c9 7-agent drill
        // confirmed this DOES drive Compose's nested-scroll consumer; the
        // missing piece was a frame barrier on resample). awaitFrame() after
        // each scroll suspends until next vsync so Compose can recompose +
        // LazyList realizes new items before walker re-samples. Replaces
        // Thread.sleep(300) which blocked the main thread (no recompose) AND
        // replaces the TD-63 MotionEvent path that leaked pointer events
        // into the next TC and broke t10/t12/t15.
        GlobalScope.launch(Dispatchers.Main) {
            try {
                val scrollable = findScrollable(activity.window.decorView)
                val composeScroll: Pair<android.view.accessibility.AccessibilityNodeProvider, Int>? =
                    if (scrollable == null) findComposeScrollable(activity.window.decorView) else null

                if (scrollable == null && composeScroll == null) {
                    error = "no scrollable view (Android or Compose)"
                    latch.countDown()
                    return@launch
                }
                val density = activity.resources.displayMetrics.density
                val viewportH = (activity.resources.displayMetrics.heightPixels / density).toInt()
                val scrollAmountPx = (viewportH * 0.7 * density).toInt()

                val schema0 = ViewTreeWalker.walk(activity)
                found = schema0.elements.firstOrNull(matchElement)

                if (found == null) {
                    for (step in 1..maxScroll) {
                        if (scrollable != null) {
                            scrollable.scrollBy(0, scrollAmountPx)
                        } else if (composeScroll != null) {
                            try {
                                composeScroll.first.performAction(
                                    composeScroll.second,
                                    android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_FORWARD,
                                    null,
                                )
                            } catch (_: Throwable) {}
                        }
                        // Two frames: one for the scroll to apply, one for
                        // LazyList to realize the now-visible items. Cheap.
                        awaitFrame()
                        awaitFrame()
                        scrollCount = step
                        val schema = ViewTreeWalker.walk(activity)
                        found = schema.elements.firstOrNull(matchElement)
                        if (found != null) break
                    }
                }

                if (restoreScroll || found == null) {
                    if (scrollable != null) {
                        scrollable.scrollTo(0, 0)
                    } else if (composeScroll != null) {
                        for (i in 0 until scrollCount) {
                            try {
                                composeScroll.first.performAction(
                                    composeScroll.second,
                                    android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD,
                                    null,
                                )
                                awaitFrame()
                            } catch (_: Throwable) { break }
                        }
                    }
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
            jsonResponse(
                """{"found":true,"element":{"id":"${escape(
                    e.id,
                )}","content":"${escape(
                    e.content ?: "",
                )}","type":"${e.type}","bounds":{"x":${e.bounds.x},"y":${e.bounds.y},"w":${e.bounds.w},"h":${e.bounds.h}},"clickable":${e.clickable},"tap_target":${e.tapTarget?.let {
                    """{"x":${it.x},"y":${it.y},"w":${it.w},"h":${it.h}}"""
                } ?: "null"}},"scrolls":$scrollCount,"scroll_restored":$scrollRestored}""",
            )
        } else {
            jsonResponse("""{"found":false,"scrolls":$scrollCount,"scroll_restored":$scrollRestored}""")
        }
    }

    private fun extractJsonBool(
        json: String,
        key: String,
    ): Boolean? =
        try {
            val obj = org.json.JSONObject(json)
            if (obj.has(key)) obj.getBoolean(key) else null
        } catch (_: Exception) {
            null
        }

    private fun extractNestedJsonString(
        json: String,
        parent: String,
        key: String,
    ): String? {
        return try {
            val obj = org.json.JSONObject(json)
            val nested = obj.optJSONObject(parent) ?: return null
            if (nested.has(key)) nested.getString(key) else null
        } catch (_: Exception) {
            null
        }
    }

    private fun extractJsonInt(
        json: String,
        key: String,
    ): Int? =
        try {
            val obj = org.json.JSONObject(json)
            if (obj.has(key)) obj.getInt(key) else null
        } catch (_: Exception) {
            null
        }

    private fun extractJsonArray(
        json: String,
        key: String,
    ): List<String>? {
        return try {
            val obj = org.json.JSONObject(json)
            val arr = obj.optJSONArray(key) ?: return null
            (0 until arr.length()).map { arr.getString(it) }
        } catch (_: Exception) {
            null
        }
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

    private fun escape(s: String): String = s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")

    private fun djb2Hash(s: String): Int {
        var hash = 5381
        for (b in s.toByteArray()) {
            hash = hash * 33 + (b.toInt() and 0xFF)
        }
        return hash
    }

    companion object {
        private var instance: SemanticServer? = null

        // TD-97: max ms to wait for SemanticAgent.loginHandler during
        // cold-init race. Override with SEMANTIC_HANDLER_WAIT_MS for tests.
        // Bumped 2000 → 5000 after the regression-android sweep on
        // 2026-06-09 (RESET_MODE=am-restart) still hit t34 r=2 with the
        // 2s budget — full process re-fork from zygote per TC pushes
        // cold-init beyond 2s on emulators. The wait is bounded and only
        // costs when the handler is genuinely null; warm-process callers
        // return on the first read.
        private val handlerWaitMs: Long =
            System.getenv("SEMANTIC_HANDLER_WAIT_MS")?.toLongOrNull() ?: 5000L

        // TD-93: gate the /debug-reset endpoint. Off by default; recipes /
        // test wrappers that need to wipe agent singleton state without
        // killing the JVM can set SEMANTIC_DEBUG=1 in the app's env.
        private val debugEnabled: Boolean =
            System.getenv("SEMANTIC_DEBUG") == "1"

        @JvmStatic
        @JvmOverloads
        fun install(
            app: Application,
            port: Int = 9876,
            gitHash: String = "",
            buildTime: String = "",
        ) {
            if (instance != null) return

            val server = SemanticServer(port, gitHash, buildTime)
            server.appRef = WeakReference(app)
            instance = server

            server.idleRegistry.register(UIThreadIdleResource())
            server.idleRegistry.register(LayoutIdleResource { server.currentActivity?.get() })
            server.idleRegistry.register(ScrollIdleResource { server.currentActivity?.get() })
            server.idleRegistry.register(ComposeIdleResource())
            server.idleRegistry.register(NetworkIdleResource(app))
            server.idleRegistry.register(DialogIdleResource { server.currentActivity?.get() })
            server.idleRegistry.register(RecyclerViewDataIdleResource { server.currentActivity?.get() })
            val activityTransition = ActivityTransitionIdleResource()
            server.idleRegistry.register(activityTransition)

            app.registerActivityLifecycleCallbacks(activityTransition)
            app.registerActivityLifecycleCallbacks(
                object : Application.ActivityLifecycleCallbacks {
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

                    override fun onActivityCreated(
                        activity: Activity,
                        savedInstanceState: Bundle?,
                    ) {}

                    override fun onActivityStarted(activity: Activity) {}

                    override fun onActivityStopped(activity: Activity) {}

                    override fun onActivitySaveInstanceState(
                        activity: Activity,
                        outState: Bundle,
                    ) {}

                    override fun onActivityDestroyed(activity: Activity) {}
                },
            )

            server.start()
            android.util.Log.i("SemanticAgent", "semantic server started on port $port")
        }

        /**
         * Register a custom idle resource with the running agent. Call this from
         * the app's init code (Application.onCreate, Hilt Module, etc.) AFTER
         * install() has run. Returns false if the agent isn't installed yet.
         *
         * The built-in NetworkIdleResource discovers OkHttp via reflection that
         * assumes Hilt + a `RestApi` component method; apps that don't match
         * that shape should register their dispatcher explicitly here.
         *
         * Example:
         *   SemanticServer.registerIdleResource("okhttp") {
         *     val d = okHttpClient.dispatcher
         *     d.runningCallsCount() == 0 && d.queuedCallsCount() == 0
         *   }
         */
        @JvmStatic
        fun registerIdleResource(name: String, isIdleFn: () -> Boolean): Boolean {
            val s = instance ?: return false
            s.idleRegistry.register(name, isIdleFn)
            return true
        }
    }
}
