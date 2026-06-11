package io.substrate.regdemo.xml

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T22Activity : AppCompatActivity() {
    private var pressed = false
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t22)
        val target = findViewById<TextView>(R.id.t22_press_target)
        val state = findViewById<TextView>(R.id.t22_state)
        state.text = "T22 Idle"
        state.contentDescription = "T22 Idle"
        target.setOnLongClickListener {
            pressed = !pressed
            val label = if (pressed) "T22 Pressed" else "T22 Idle"
            state.text = label
            state.contentDescription = label
            true
        }
    }
}
