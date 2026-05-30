package dev.substrate.semantic

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import java.util.concurrent.ConcurrentHashMap

data class MockRule(val urlPattern: String, val method: String, val response: MockResponse)
data class MockResponse(val status: Int, val body: String, val headers: Map<String, String> = emptyMap())

class MockRegistry {
    private val rules = ConcurrentHashMap<String, MockRule>()

    fun canHandle(url: String, method: String): Boolean {
        return rules.values.any { rule ->
            url.contains(rule.urlPattern) && rule.method.equals(method, ignoreCase = true)
        }
    }

    fun handle(url: String, method: String, requestBody: String? = null): MockResponse? {
        return rules.values.firstOrNull { rule ->
            url.contains(rule.urlPattern) && rule.method.equals(method, ignoreCase = true)
        }?.response
    }

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
}

class MockInterceptor(private val registry: MockRegistry) : okhttp3.Interceptor {
    override fun intercept(chain: okhttp3.Interceptor.Chain): okhttp3.Response {
        val request = chain.request()
        val url = request.url.toString()
        val method = request.method

        if (url.contains("/semantic") || url.contains("/mock") || url.contains("/unmock")) {
            return chain.proceed(request)
        }

        val mockResponse = registry.handle(url, method)
        if (mockResponse != null) {
            val responseBuilder = okhttp3.Response.Builder()
                .request(request)
                .protocol(okhttp3.Protocol.HTTP_1_1)
                .code(mockResponse.status)
                .message("Mocked")
                .body(okhttp3.ResponseBody.create(
                    (mockResponse.headers["Content-Type"] ?: "application/json").toMediaTypeOrNull(),
                    mockResponse.body
                ))
            for ((key, value) in mockResponse.headers) {
                responseBuilder.addHeader(key, value)
            }
            return responseBuilder.build()
        }

        return chain.proceed(request)
    }
}
