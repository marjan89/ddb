package io.substrate.regdemoxml

import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T9Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t9)
        val delayed = findViewById<TextView>(R.id.t9_delayed)
        delayed.text = ""
        delayed.contentDescription = "t9-placeholder"
        findViewById<Button>(R.id.t9_trigger).setOnClickListener {
            delayed.text = ""
            delayed.contentDescription = "t9-placeholder"
            Handler(Looper.getMainLooper()).postDelayed({
                delayed.text = "T9 Delayed Element"
                delayed.contentDescription = "T9 Delayed Element"
            }, 3000)
        }
    }
}
