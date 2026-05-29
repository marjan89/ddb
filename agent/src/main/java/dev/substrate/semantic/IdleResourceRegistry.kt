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
    override fun isIdle(): Boolean {
        if (Looper.myLooper() == Looper.getMainLooper()) return true
        val idle = AtomicBoolean(false)
        val latch = CountDownLatch(1)
        Handler(Looper.getMainLooper()).post {
            Looper.myQueue().addIdleHandler(object : MessageQueue.IdleHandler {
                override fun queueIdle(): Boolean {
                    idle.set(true)
                    latch.countDown()
                    return false
                }
            })
        }
        return latch.await(2, TimeUnit.SECONDS) && idle.get()
    }
}

class LayoutIdleResource(private val activityProvider: () -> android.app.Activity?) : IdleResource {
    override val name = "layout"
    override fun isIdle(): Boolean {
        val activity = activityProvider() ?: return true
        val idle = AtomicBoolean(false)
        val latch = CountDownLatch(1)
        Handler(Looper.getMainLooper()).post {
            val rootView = activity.window.decorView
            idle.set(!rootView.isLayoutRequested && !rootView.isDirty)
            latch.countDown()
        }
        return latch.await(2, TimeUnit.SECONDS) && idle.get()
    }
}

class ScrollIdleResource(private val activityProvider: () -> android.app.Activity?) : IdleResource {
    override val name = "scroll"
    override fun isIdle(): Boolean {
        val activity = activityProvider() ?: return true
        val idle = AtomicBoolean(true)
        val latch = CountDownLatch(1)
        Handler(Looper.getMainLooper()).post {
            idle.set(isScrollIdle(activity.window.decorView))
            latch.countDown()
        }
        return latch.await(2, TimeUnit.SECONDS) && idle.get()
    }

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

class NetworkIdleResource : IdleResource {
    override val name = "network"
    private val inFlightCount = java.util.concurrent.atomic.AtomicInteger(0)

    fun onRequestStarted() { inFlightCount.incrementAndGet() }
    fun onRequestFinished() { inFlightCount.decrementAndGet() }

    override fun isIdle(): Boolean = inFlightCount.get() == 0
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
