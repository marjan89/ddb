package io.substrate.regdemo.xml

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import androidx.appcompat.app.AppCompatActivity

class T11Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t11)
        findViewById<Button>(R.id.t11_l1_button).setOnClickListener {
            startActivity(Intent(this, T11Level1Activity::class.java))
        }
    }
}

class T11Level1Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t11_l1)
        findViewById<Button>(R.id.t11_l2_button).setOnClickListener {
            startActivity(Intent(this, T11Level2Activity::class.java))
        }
    }
}

class T11Level2Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t11_l2)
    }
}
