package dev.substrate.semantic

/**
 * Public agent API surface for host-app integration. Mirror of the iOS
 * `SemanticAgent` companion API in `device-control-ios/semantic-agent`.
 *
 * Use:
 *   SemanticAgent.loginHandler = { email, password, completion ->
 *       authService.login(email, password) { ok, error ->
 *           completion(ok, error)
 *       }
 *   }
 *
 * The agent's POST /login route invokes this handler with the parsed
 * credentials and reports the completion result back over HTTP.
 * Clear by setting to null in onDispose / Activity teardown.
 */
object SemanticAgent {
    /**
     * Login delegation handler. Receives email + password from the
     * /login HTTP body, must invoke completion(success, error?) when done.
     * Null means "no handler registered" — /login responds 503.
     */
    @Volatile
    @JvmField
    var loginHandler: ((email: String, password: String, completion: (Boolean, String?) -> Unit) -> Unit)? = null
}
