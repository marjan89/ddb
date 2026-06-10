package dev.substrate.semantic

import java.io.ByteArrayInputStream
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLConnection
import java.net.URLStreamHandler
import java.net.URLStreamHandlerFactory

/**
 * TD-69: JVM-wide URLStreamHandlerFactory that intercepts http/https
 * URL.openConnection() calls when MockRegistry has a matching rule.
 * Installed once via SemanticAgent.installUrlInterceptor(); subsequent
 * setURLStreamHandlerFactory calls would throw (JVM contract: one-shot per
 * process), so we guard with a static flag.
 *
 * OkHttp does NOT route through this factory — hosts must install
 * MockRegistry.shared.interceptor on their OkHttpClient.Builder() for OkHttp
 * traffic. This shim only covers HttpURLConnection / URL.openConnection().
 */
internal class MockUrlStreamHandlerFactory : URLStreamHandlerFactory {
    override fun createURLStreamHandler(protocol: String): URLStreamHandler? {
        if (protocol != "http" && protocol != "https") return null
        return MockUrlStreamHandler(protocol)
    }
}

private class MockUrlStreamHandler(private val protocol: String) : URLStreamHandler() {
    override fun openConnection(url: URL): URLConnection {
        val urlStr = url.toString()
        // Defer to default handler for non-mocked URLs by re-opening through
        // the JVM's built-in handler chain (we can't recurse via URL.openConnection
        // because that would loop into this factory).
        val rule = MockRegistry.shared.handle(urlStr, "GET")
            ?: MockRegistry.shared.handle(urlStr, "POST")
            ?: MockRegistry.shared.handle(urlStr, "PUT")
            ?: MockRegistry.shared.handle(urlStr, "DELETE")
            ?: MockRegistry.shared.handle(urlStr, "PATCH")
        if (rule == null) {
            return passthrough(url)
        }
        return MockedHttpURLConnection(url, rule)
    }

    private fun passthrough(url: URL): URLConnection {
        // Re-resolve using the platform's default handler. We construct a new
        // URL with an explicit default handler reference via reflection on the
        // sun.net protocol handler — fallback chain.
        return try {
            val defaultHandlerClass = Class.forName(
                if (protocol == "https") "com.android.okhttp.HttpsHandler" else "com.android.okhttp.HttpHandler",
            )
            val handler = defaultHandlerClass.getDeclaredConstructor().newInstance() as URLStreamHandler
            val ctor = URL::class.java.getDeclaredConstructor(
                URL::class.java, String::class.java, URLStreamHandler::class.java,
            )
            ctor.isAccessible = true
            val u2 = ctor.newInstance(null, url.toString(), handler)
            // Invoke the protected openConnection(URL) via reflection on the handler.
            val m = URLStreamHandler::class.java.getDeclaredMethod("openConnection", URL::class.java)
            m.isAccessible = true
            m.invoke(handler, u2) as URLConnection
        } catch (t: Throwable) {
            android.util.Log.w("MockUrlHandler", "passthrough failed for $url: ${t.message}; returning mock-404")
            MockedHttpURLConnection(url, MockResponse(status = 404, body = ""))
        }
    }
}

private class MockedHttpURLConnection(url: URL, private val mock: MockResponse) : HttpURLConnection(url) {
    private val bodyBytes: ByteArray = mock.body.toByteArray(Charsets.UTF_8)

    override fun connect() {}
    override fun disconnect() {}
    override fun usingProxy(): Boolean = false
    override fun getResponseCode(): Int = mock.status
    override fun getResponseMessage(): String = "Mocked"
    override fun getInputStream(): InputStream = ByteArrayInputStream(bodyBytes)
    override fun getErrorStream(): InputStream? = null
    override fun getContentLength(): Int = bodyBytes.size
    override fun getContentType(): String = mock.headers["Content-Type"] ?: "application/json"
    override fun getHeaderField(name: String): String? = mock.headers[name]
}
