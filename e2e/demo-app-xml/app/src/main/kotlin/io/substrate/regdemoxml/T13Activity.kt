package io.substrate.regdemo.xml

import android.os.Bundle
import android.view.View
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T13Activity : AppCompatActivity() {
    private val listener: (T13State) -> Unit = { render(it) }
    private lateinit var primary: TextView
    private lateinit var error: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t13)
        primary = findViewById(R.id.t13_primary)
        error = findViewById(R.id.t13_error)
        findViewById<Button>(R.id.t13_reset).setOnClickListener { T13Store.reset() }
        T13Store.observe(listener)
    }

    override fun onDestroy() {
        T13Store.unobserve(listener)
        super.onDestroy()
    }

    private fun render(s: T13State) {
        val label = when (s) {
            T13State.Unlocked -> "T13 Unlocked"
            else -> "T13 Locked"
        }
        primary.text = label
        primary.contentDescription = label
        error.visibility = if (s == T13State.Error) View.VISIBLE else View.GONE
    }
}
