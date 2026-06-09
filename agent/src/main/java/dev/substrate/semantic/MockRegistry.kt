package dev.substrate.semantic

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import java.util.concurrent.ConcurrentHashMap

data class MockRule(
    val urlPattern: String,
    val method: String,
    val response: MockResponse,
)

data class MockResponse(
    val status: Int,
    val body: String,
    val headers: Map<String, String> = emptyMap(),
)

class MockRegistry {
    companion object {
        @JvmStatic
        val shared = MockRegistry()
    }

    val interceptor: MockInterceptor by lazy { MockInterceptor(this) }
    private val rules = ConcurrentHashMap<String, MockRule>()

    fun canHandle(
        url: String,
        method: String,
    ): Boolean =
        rules.values.any { rule ->
            url.contains(rule.urlPattern) && rule.method.equals(method, ignoreCase = true)
        }

    fun handle(
        url: String,
        method: String,
        requestBody: String? = null,
    ): MockResponse? =
        rules.values
            .firstOrNull { rule ->
                url.contains(rule.urlPattern) && rule.method.equals(method, ignoreCase = true)
            }?.response

    fun register(mocks: List<MockRule>) {
        for (rule in mocks) {
            rules["${rule.method}:${rule.urlPattern}"] = rule
        }
    }

    fun clear() {
        rules.clear()
    }

    fun clear(urlPattern: String) {
        rules.entries.removeIf { it.value.urlPattern == urlPattern }
    }

    fun isEmpty(): Boolean = rules.isEmpty()

    fun ruleCount(): Int = rules.size
}

class MockInterceptor(
    private val registry: MockRegistry,
) : okhttp3.Interceptor {
    @Volatile var hitCount = 0
        private set

    override fun intercept(chain: okhttp3.Interceptor.Chain): okhttp3.Response {
        val request = chain.request()
        val url = request.url.toString()
        val method = request.method

        if (url.contains("/semantic") || url.contains("/mock") || url.contains("/unmock") || url.contains("/health") ||
            url.contains("/version")
        ) {
            return chain.proceed(request)
        }

        val mockResponse = registry.handle(url, method)
        if (mockResponse != null) {
            hitCount++
            android.util.Log.i("MockInterceptor", "INTERCEPTED $method $url → ${mockResponse.status} (hit #$hitCount)")
            val responseBuilder =
                okhttp3.Response
                    .Builder()
                    .request(request)
                    .protocol(okhttp3.Protocol.HTTP_1_1)
                    .code(mockResponse.status)
                    .message("Mocked")
                    .body(
                        okhttp3.ResponseBody.create(
                            (mockResponse.headers["Content-Type"] ?: "application/json").toMediaTypeOrNull(),
                            mockResponse.body,
                        ),
                    )
            for ((key, value) in mockResponse.headers) {
                responseBuilder.addHeader(key, value)
            }
            return responseBuilder.build()
        }

        return chain.proceed(request)
    }
}
