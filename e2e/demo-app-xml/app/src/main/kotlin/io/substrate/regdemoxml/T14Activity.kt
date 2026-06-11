package io.substrate.regdemo.xml

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import androidx.appcompat.app.AppCompatActivity

class T14Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t14)
        findViewById<Button>(R.id.t14_open_anchor).setOnClickListener {
            startActivity(Intent(this, T14AnchorActivity::class.java))
        }
    }
}

class T14AnchorActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t14_anchor)
        findViewById<Button>(R.id.t14_back).setOnClickListener { finish() }
    }
}
