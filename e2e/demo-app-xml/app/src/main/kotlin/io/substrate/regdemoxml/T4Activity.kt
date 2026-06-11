package io.substrate.regdemo.xml

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import androidx.appcompat.app.AppCompatActivity

class T4Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t4)
        findViewById<Button>(R.id.t4_go_detail).setOnClickListener {
            startActivity(Intent(this, T4DetailActivity::class.java))
        }
    }
}

class T4DetailActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t4_detail)
        findViewById<Button>(R.id.t4_go_back).setOnClickListener { finish() }
    }
}
