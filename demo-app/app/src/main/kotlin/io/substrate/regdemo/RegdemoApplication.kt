package io.substrate.regdemo

import android.app.Application
import dev.substrate.semantic.SemanticAgent

class RegdemoApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        SemanticAgent.loginHandler = { email, password, completion ->
            T13Store.handle(email, password, completion)
        }
    }
}
