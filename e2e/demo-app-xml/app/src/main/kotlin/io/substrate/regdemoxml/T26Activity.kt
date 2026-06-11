package io.substrate.regdemoxml

import android.os.Bundle
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T26Activity : AppCompatActivity() {
    private var visible = true
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t26)
        val target = findViewById<TextView>(R.id.t26_target)
        fun apply() {
            val label = if (visible) "T26 Target" else ""
            target.text = label
            target.contentDescription = if (label.isEmpty()) "t26-hidden" else label
        }
        apply()
        findViewById<Button>(R.id.t26_toggle).setOnClickListener {
            visible = !visible
            apply()
        }
    }
}
