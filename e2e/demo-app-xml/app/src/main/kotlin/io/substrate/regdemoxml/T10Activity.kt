package io.substrate.regdemo.xml

import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import java.net.HttpURLConnection
import java.net.URL

class T10Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t10)
        val output = findViewById<TextView>(R.id.t10_output)
        output.text = "T10 Awaiting"
        output.contentDescription = "T10 Awaiting"
        findViewById<Button>(R.id.t10_fetch).setOnClickListener {
            Thread {
                val text = try {
                    val url = URL("https://api.example.com/t10")
                    val conn = url.openConnection() as HttpURLConnection
                    val body = conn.inputStream.bufferedReader().use { it.readText() }
                    conn.disconnect()
                    body.ifEmpty { "T10 Empty" }
                } catch (t: Throwable) {
                    "T10 Error: ${t.message}"
                }
                Handler(Looper.getMainLooper()).post {
                    output.text = text
                    output.contentDescription = text
                }
            }.start()
        }
    }
}
