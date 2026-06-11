package io.substrate.regdemo.xml

import android.os.Bundle
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T15Activity : AppCompatActivity() {
    private var disabled = false
    private var taps = 0
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t15)
        val action = findViewById<Button>(R.id.t15_action)
        val tapsView = findViewById<TextView>(R.id.t15_taps)
        val stateView = findViewById<TextView>(R.id.t15_state_label)
        val toggle = findViewById<Button>(R.id.t15_toggle)
        fun applyState() {
            action.isEnabled = !disabled
            // Android View.isEnabled does NOT auto-toggle isClickable; the
            // runner's element_state assertion (test.rs:2221) checks the
            // walker's `clickable` field (= view.isClickable) as the proxy
            // for "enabled". Keep them in sync so the assertion contract
            // holds (Compose Button collapses both into one knob).
            action.isClickable = !disabled
            val label = if (disabled) "T15 disabled state" else "T15 enabled state"
            stateView.text = label
            stateView.contentDescription = label
        }
        action.setOnClickListener {
            taps++
            tapsView.text = "taps=$taps"
            tapsView.contentDescription = "taps=$taps"
        }
        toggle.setOnClickListener {
            disabled = !disabled
            applyState()
        }
        applyState()
    }
}
