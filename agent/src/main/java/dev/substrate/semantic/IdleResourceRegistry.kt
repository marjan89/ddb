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
            Looper.myQueue().addIdleHandler(object : MessageQueue.IdleHandler {
                override fun queueIdle(): Boolean {
                    lastIdle = true
                    lastCheck = System.currentTimeMillis()
                    return false
                }
            })
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

class LayoutIdleResource(private val activityProvider: () -> android.app.Activity?) : IdleResource {
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

class ScrollIdleResource(private val activityProvider: () -> android.app.Activity?) : IdleResource {
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
            } catch (_: Exception) {}
        }
        if (view is android.view.ViewGroup) {
            for (i in 0 until view.childCount) {
                if (!isScrollIdle(view.getChildAt(i))) return false
            }
        }
        return true
    }
}

class NetworkIdleResource(private val appContext: android.content.Context) : IdleResource {
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
                    val handler = java.lang.reflect.Proxy.getInvocationHandler(restApi)
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
        } catch (_: Exception) {}
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

class DialogIdleResource(private val activityProvider: () -> android.app.Activity?) : IdleResource {
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

class ActivityTransitionIdleResource : IdleResource, android.app.Application.ActivityLifecycleCallbacks {
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

    override fun onActivityCreated(activity: android.app.Activity, savedInstanceState: android.os.Bundle?) {}
    override fun onActivityStarted(activity: android.app.Activity) {}
    override fun onActivityStopped(activity: android.app.Activity) {}
    override fun onActivitySaveInstanceState(activity: android.app.Activity, outState: android.os.Bundle) {}
    override fun onActivityDestroyed(activity: android.app.Activity) {}
}

class IdleResourceRegistry {
    private val resources = ConcurrentHashMap<String, IdleResource>()

    fun register(resource: IdleResource) {
        resources[resource.name] = resource
    }

    fun unregister(name: String) {
        resources.remove(name)
    }

    fun isIdle(resourceNames: List<String>? = null): Boolean {
        val targets = if (resourceNames.isNullOrEmpty()) {
            resources.values
        } else {
            resourceNames.mapNotNull { resources[it] }
        }
        return targets.all { it.isIdle() }
    }

    fun waitForIdle(resourceNames: List<String>? = null, timeoutMs: Long = 5000): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (isIdle(resourceNames)) return true
            Thread.sleep(100)
        }
        return false
    }

    fun registeredNames(): List<String> = resources.keys().toList()
}
