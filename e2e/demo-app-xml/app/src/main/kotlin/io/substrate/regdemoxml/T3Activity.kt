package io.substrate.regdemoxml

import android.os.Bundle
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T3Activity : AppCompatActivity() {
    private var count = 0
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t3)
        val counter = findViewById<TextView>(R.id.t3_count)
        findViewById<Button>(R.id.t3_increment).setOnClickListener {
            count++
            counter.text = count.toString()
        }
    }
}
