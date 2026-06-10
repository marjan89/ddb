package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import dev.substrate.semantic.MockRegistry
import okhttp3.OkHttpClient
import okhttp3.Request

@Composable
fun T10MockURLScreen() {
    var response by remember { mutableStateOf("T10 Awaiting") }
    var trigger by remember { mutableIntStateOf(0) }
    var loading by remember { mutableStateOf(false) }
    val client = remember {
        OkHttpClient.Builder()
            .addInterceptor(MockRegistry.shared.interceptor)
            .build()
    }
    LaunchedEffect(trigger) {
        if (trigger == 0) return@LaunchedEffect
        loading = true
        val text = withContext(Dispatchers.IO) {
            try {
                val req = Request.Builder().url("https://api.example.com/t10").get().build()
                client.newCall(req).execute().use { resp ->
                    resp.body?.string() ?: "T10 Empty"
                }
            } catch (t: Throwable) {
                "T10 Error: ${t.message}"
            }
        }
        response = text
        loading = false
    }
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T10 Mock")
        Button(onClick = { if (!loading) trigger++ }, enabled = !loading) {
            Text("T10 Fetch")
        }
        Text(response, modifier = Modifier.semantics { contentDescription = response })
    }
}
