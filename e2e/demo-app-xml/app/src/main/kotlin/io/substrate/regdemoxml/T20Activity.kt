package io.substrate.regdemoxml

import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout

class T20Activity : AppCompatActivity() {
    private var counter = 0
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t20)
        val refresh = findViewById<SwipeRefreshLayout>(R.id.t20_refresh)
        val text = findViewById<TextView>(R.id.t20_counter)
        text.text = "T20 Counter $counter"
        text.contentDescription = "T20 Counter $counter"
        refresh.setOnRefreshListener {
            counter++
            text.text = "T20 Counter $counter"
            text.contentDescription = "T20 Counter $counter"
            Handler(Looper.getMainLooper()).postDelayed({ refresh.isRefreshing = false }, 400)
        }
    }
}
