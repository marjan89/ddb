package io.substrate.regdemoxml

import android.app.Application
import android.os.Handler
import android.os.Looper
import dev.substrate.semantic.SemanticAgent

class RegdemoXmlApplication : Application() {
    private val mainHandler = Handler(Looper.getMainLooper())

    override fun onCreate() {
        super.onCreate()
        SemanticAgent.loginHandler = { email, password, completion ->
            mainHandler.post {
                T13Store.handle(email, password, completion)
            }
        }
    }
}
