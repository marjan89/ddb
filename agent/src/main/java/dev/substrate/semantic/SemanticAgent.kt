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

    @Volatile
    private var urlInterceptorInstalled = false

    /**
     * TD-69: install a JVM-wide URLStreamHandlerFactory that intercepts
     * http/https URL.openConnection() calls when MockRegistry has a matching
     * rule. JVM contract: setURLStreamHandlerFactory is one-shot per process —
     * subsequent calls throw. Call from Application.onCreate BEFORE any host
     * code touches java.net.URL. No-op on second invocation.
     *
     * Does NOT cover OkHttp — OkHttp uses raw sockets, not URLConnection.
     * Hosts using OkHttp must add MockRegistry.shared.interceptor to their
     * OkHttpClient.Builder.
     */
    @JvmStatic
    fun installUrlInterceptor() {
        if (urlInterceptorInstalled) return
        try {
            java.net.URL.setURLStreamHandlerFactory(MockUrlStreamHandlerFactory())
            urlInterceptorInstalled = true
        } catch (e: Error) {
            android.util.Log.w(
                "SemanticAgent",
                "URLStreamHandlerFactory already set by another component; mock interception unavailable for HttpURLConnection. ${e.message}",
            )
        }
    }
}
