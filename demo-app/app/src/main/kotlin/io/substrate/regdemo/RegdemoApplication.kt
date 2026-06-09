package io.substrate.regdemo

import android.app.Application
import android.os.Handler
import android.os.Looper
import dev.substrate.semantic.SemanticAgent

class RegdemoApplication : Application() {
    private val mainHandler = Handler(Looper.getMainLooper())

    override fun onCreate() {
        super.onCreate()
        // TD-93 Bug 1 fix: SemanticAgent.loginHandler is invoked from NanoHTTPD's
        // HTTP worker thread (SemanticServer.handleLogin). T13Store.handle()
        // mutates `state.value = T13State.Unlocked` which is a Compose
        // mutableStateOf write; Compose requires main-thread mutations to
        // reliably trigger recomposition. Off-main mutations sometimes work
        // (Compose is lenient) and sometimes don't (race with frame schedule)
        // — that's the t34 indicator-propagation intermittent. Marshal to the
        // main looper at the boundary so the rest of the handler runs in the
        // correct context. completion(...) lands on the main thread too,
        // which is harmless (NanoHTTPD's latch-await is thread-safe).
        SemanticAgent.loginHandler = { email, password, completion ->
            mainHandler.post {
                T13Store.handle(email, password, completion)
            }
        }
    }
}
