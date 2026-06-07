package dev.substrate.semantic

import android.os.Handler
import android.os.Looper
import android.os.MessageQueue
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

interface IdleResource {
    val name: String

    fun isIdle(): Boolean
}

class UIThreadIdleResource : IdleResource {
    override val name = "ui_thread"

    @Volatile private var lastIdle = false

    @Volatile private var lastCheck = 0L
    private val handler = Handler(Looper.getMainLooper())

    init {
        schedulePoll()
    }

    private fun schedulePoll() {
        handler.postDelayed({
            Looper.myQueue().addIdleHandler(
                object : MessageQueue.IdleHandler {
                    override fun queueIdle(): Boolean {
                        lastIdle = true
                        lastCheck = System.currentTimeMillis()
                        return false
                    }
                },
            )
            handler.postDelayed({
                if (System.currentTimeMillis() - lastCheck > 150) {
                    lastIdle = false
                }
                schedulePoll()
            }, 200)
        }, 100)
    }

    override fun isIdle(): Boolean = lastIdle
}

class LayoutIdleResource(
    private val activityProvider: () -> android.app.Activity?,
) : IdleResource {
    override val name = "layout"

    @Volatile private var stableCount = 0
    private val handler = Handler(Looper.getMainLooper())

    init {
        schedulePoll()
    }

    private fun schedulePoll() {
        handler.postDelayed({
            val activity = activityProvider()
            if (activity != null) {
                val rootView = activity.window.decorView
                val clean = !rootView.isLayoutRequested && !rootView.isDirty
                if (clean) stableCount++ else stableCount = 0
            }
            schedulePoll()
        }, 100)
    }

    override fun isIdle(): Boolean = stableCount >= 3
}

class ScrollIdleResource(
    private val activityProvider: () -> android.app.Activity?,
) : IdleResource {
    override val name = "scroll"

    @Volatile private var lastIdle = true
    private val handler = Handler(Looper.getMainLooper())

    init {
        schedulePoll()
    }

    private fun schedulePoll() {
        handler.postDelayed({
            val activity = activityProvider()
            lastIdle = if (activity != null) isScrollIdle(activity.window.decorView) else true
            schedulePoll()
        }, 100)
    }

    override fun isIdle(): Boolean = lastIdle

    private fun isScrollIdle(view: android.view.View): Boolean {
        if (view.javaClass.name.contains("RecyclerView")) {
            try {
                val m = view.javaClass.getMethod("getScrollState")
                val state = m.invoke(view) as Int
                if (state != 0) return false
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
}

class NetworkIdleResource(
    private val appContext: android.content.Context,
) : IdleResource {
    override val name = "network"

    @Volatile private var lastIdle = true
    private var dispatcher: okhttp3.Dispatcher? = null
    private var discoveryAttempted = false

    private fun discoverDispatcher(): okhttp3.Dispatcher? {
        if (discoveryAttempted) return dispatcher
        discoveryAttempted = true
        try {
            val app = appContext.applicationContext
            val componentMethod = app.javaClass.getMethod("generatedComponent")
            val component = componentMethod.invoke(app)
            for (m in component.javaClass.methods) {
                if (m.parameterCount == 0 && m.returnType.name.contains("RestApi")) {
                    val restApi = m.invoke(component)
                    val handler =
                        java.lang.reflect.Proxy
                            .getInvocationHandler(restApi)
                    val retrofitField = handler.javaClass.getDeclaredField("retrofit")
                    retrofitField.isAccessible = true
                    val retrofit = retrofitField.get(handler) as retrofit2.Retrofit
                    val factoryField = retrofit2.Retrofit::class.java.getDeclaredField("callFactory")
                    factoryField.isAccessible = true
                    val client = factoryField.get(retrofit) as okhttp3.OkHttpClient
                    dispatcher = client.dispatcher
                    break
                }
            }
        } catch (_: Exception) {
        }
        return dispatcher
    }

    override fun isIdle(): Boolean {
        val d = discoverDispatcher()
        if (d != null) {
            lastIdle = d.runningCallsCount() == 0 && d.queuedCallsCount() == 0
        }
        return lastIdle
    }
}

class DialogIdleResource(
    private val activityProvider: () -> android.app.Activity?,
) : IdleResource {
    override val name = "dialog"

    @Volatile private var windowCount = 1

    @Volatile private var stableSince = System.currentTimeMillis()
    private var lastCount = 1
    private val handler = Handler(Looper.getMainLooper())

    init {
        schedulePoll()
    }

    private fun schedulePoll() {
        handler.postDelayed({
            val count = countWindows()
            if (count != lastCount) {
                lastCount = count
                stableSince = System.currentTimeMillis()
            }
            windowCount = count
            schedulePoll()
        }, 100)
    }

    private fun countWindows(): Int {
        try {
            val wmgClass = Class.forName("android.view.WindowManagerGlobal")
            val getInstance = wmgClass.getMethod("getInstance")
            val wmg = getInstance.invoke(null)
            val viewsField = wmgClass.getDeclaredField("mViews")
            viewsField.isAccessible = true
            val views = viewsField.get(wmg) as? java.util.ArrayList<*>
            return views?.size ?: 1
        } catch (_: Exception) {
            return 1
        }
    }

    override fun isIdle(): Boolean {
        // idle = single window (no dialog) AND stable for 300ms
        return windowCount <= 1 || (System.currentTimeMillis() - stableSince) >= 300
    }
}

class ActivityTransitionIdleResource :
    IdleResource,
    android.app.Application.ActivityLifecycleCallbacks {
    override val name = "activity_transition"

    @Volatile private var transitioning = false

    @Volatile private var transitionStart = 0L

    override fun onActivityPaused(activity: android.app.Activity) {
        transitioning = true
        transitionStart = System.currentTimeMillis()
    }

    override fun onActivityResumed(activity: android.app.Activity) {
        transitioning = false
    }

    override fun isIdle(): Boolean {
        if (!transitioning) return true
        // force idle after 5s to prevent permanent hang
        return (System.currentTimeMillis() - transitionStart) > 5000
    }

    override fun onActivityCreated(
        activity: android.app.Activity,
        savedInstanceState: android.os.Bundle?,
    ) {}

    override fun onActivityStarted(activity: android.app.Activity) {}

    override fun onActivityStopped(activity: android.app.Activity) {}

    override fun onActivitySaveInstanceState(
        activity: android.app.Activity,
        outState: android.os.Bundle,
    ) {}

    override fun onActivityDestroyed(activity: android.app.Activity) {}
}

class RecyclerViewDataIdleResource(
    private val activityProvider: () -> android.app.Activity?,
) : IdleResource {
    override val name = "recycler_data"

    @Volatile private var lastChangeTime = 0L
    private val handler = Handler(Looper.getMainLooper())
    private val trackedAdapters = java.util.Collections.newSetFromMap(
        java.util.WeakHashMap<androidx.recyclerview.widget.RecyclerView.Adapter<*>, Boolean>()
    )

    init {
        scheduleScan()
    }

    private fun scheduleScan() {
        handler.postDelayed({
            try {
                val activity = activityProvider()
                if (activity != null) {
                    scanForRecyclerViews(activity.window.decorView)
                }
            } catch (_: Exception) {}
            scheduleScan()
        }, 500)
    }

    private fun scanForRecyclerViews(view: android.view.View) {
        if (view is androidx.recyclerview.widget.RecyclerView) {
            val adapter = view.adapter
            if (adapter != null && trackedAdapters.add(adapter)) {
                try {
                    adapter.registerAdapterDataObserver(object : androidx.recyclerview.widget.RecyclerView.AdapterDataObserver() {
                        override fun onChanged() { lastChangeTime = System.currentTimeMillis() }
                        override fun onItemRangeInserted(p: Int, c: Int) { lastChangeTime = System.currentTimeMillis() }
                        override fun onItemRangeRemoved(p: Int, c: Int) { lastChangeTime = System.currentTimeMillis() }
                        override fun onItemRangeChanged(p: Int, c: Int) { lastChangeTime = System.currentTimeMillis() }
                        override fun onItemRangeMoved(f: Int, t: Int, c: Int) { lastChangeTime = System.currentTimeMillis() }
                    })
                } catch (_: Exception) {}
            }
        }
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                scanForRecyclerViews(view.getChildAt(i))
            }
        }
    }

    override fun isIdle(): Boolean {
        if (lastChangeTime == 0L) return true
        return (System.currentTimeMillis() - lastChangeTime) > 200
    }
}

/**
 * TD-51: Compose-aware idle resource. Reads Recomposer state reflectively
 * so the agent does not hard-depend on androidx.compose.runtime. For
 * non-Compose apps the class lookup fails once and the resource reports
 * idle=true (no Compose work in flight ever).
 *
 * Recomposer.Companion.runningRecomposers : StateFlow<Set<Recomposer>>
 * Recomposer.currentState : StateFlow<Recomposer.State>
 * State.PendingWork = not idle. All other states (Idle, Inactive,
 * InactivePendingWork during teardown, ShuttingDown, ShutDown) = idle.
 *
 * 30ms grace window after last non-idle observation guards against the
 * race where Compose flips to Idle but the AccessibilityNodeInfo tree
 * has not yet propagated the recomposed nodes (TD-51 root cause).
 */
class ComposeIdleResource : IdleResource {
    override val name = "compose"

    @Volatile private var available = true
    @Volatile private var lastNonIdleMs = 0L
    private val graceMs = 30L

    private val recomposerClass: Class<*>? by lazy {
        try {
            Class.forName("androidx.compose.runtime.Recomposer")
        } catch (_: Throwable) {
            available = false
            null
        }
    }

    private val pendingWorkOrdinal: Int by lazy {
        try {
            val stateClass = Class.forName("androidx.compose.runtime.Recomposer\$State")
            @Suppress("UNCHECKED_CAST")
            val values = (stateClass.getMethod("values").invoke(null) as Array<Enum<*>>)
            val pw = values.firstOrNull { it.name == "PendingWork" }
            pw?.ordinal ?: 5
        } catch (_: Throwable) {
            5
        }
    }

    override fun isIdle(): Boolean {
        val cls = recomposerClass ?: return true
        if (!available) return true
        return try {
            val companion = cls.getField("Companion").get(null)
            val flow = companion.javaClass.getMethod("getRunningRecomposers").invoke(companion)
            @Suppress("UNCHECKED_CAST")
            val recomposers = flow.javaClass.getMethod("getValue").invoke(flow) as Set<Any>
            if (recomposers.isEmpty()) return true
            val now = System.currentTimeMillis()
            for (r in recomposers) {
                val stateFlow = r.javaClass.getMethod("getCurrentState").invoke(r)
                val state = stateFlow.javaClass.getMethod("getValue").invoke(stateFlow) as Enum<*>
                if (state.ordinal == pendingWorkOrdinal) {
                    lastNonIdleMs = now
                    return false
                }
            }
            // No recomposer in PendingWork. Apply grace window so a11y
            // tree has time to absorb the recomposition.
            (now - lastNonIdleMs) >= graceMs
        } catch (t: Throwable) {
            android.util.Log.w("SemanticAgent", "ComposeIdleResource reflective probe failed: ${t.message}")
            available = false
            true
        }
    }
}

class IdleResourceRegistry {
    private val resources = ConcurrentHashMap<String, IdleResource>()

    fun register(resource: IdleResource) {
        resources[resource.name] = resource
    }

    /**
     * Convenience overload: register a lambda-backed IdleResource by name.
     * Apps that can't rely on the built-in NetworkIdleResource reflection
     * (non-Hilt projects, custom RestApi shapes, etc.) should register their
     * OkHttp dispatcher idle check this way at app init.
     *
     * Example:
     *   IdleResourceRegistry.register("okhttp") {
     *     okHttpClient.dispatcher.runningCallsCount() == 0
     *       && okHttpClient.dispatcher.queuedCallsCount() == 0
     *   }
     *
     * Re-registering the same name replaces the prior resource.
     */
    fun register(name: String, isIdleFn: () -> Boolean) {
        val resourceName = name
        register(object : IdleResource {
            override val name: String = resourceName
            override fun isIdle(): Boolean = isIdleFn()
        })
    }

    fun unregister(name: String) {
        resources.remove(name)
    }

    /**
     * Invoke a resource's isIdle() under a Throwable guard. A misbehaving
     * caller-supplied lambda must not be able to crash the /idle HTTP handler
     * (which would close the connection and starve every consumer). Default
     * on failure is `false` (busy) so callers err on the side of waiting,
     * never on the side of false-idle.
     */
    private fun safeIsIdle(resource: IdleResource): Boolean {
        return try {
            resource.isIdle()
        } catch (t: Throwable) {
            android.util.Log.w(
                "SemanticAgent",
                "idle resource '${resource.name}' threw on isIdle(): ${t.message}",
                t,
            )
            false
        }
    }

    fun isIdle(resourceNames: List<String>? = null): Boolean {
        val targets =
            if (resourceNames.isNullOrEmpty()) {
                resources.values
            } else {
                resourceNames.mapNotNull { resources[it] }
            }
        return targets.all { safeIsIdle(it) }
    }

    fun waitForIdle(
        resourceNames: List<String>? = null,
        timeoutMs: Long = 5000,
    ): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (isIdle(resourceNames)) return true
            Thread.sleep(100)
        }
        return false
    }

    fun registeredNames(): List<String> = resources.keys().toList()
}
