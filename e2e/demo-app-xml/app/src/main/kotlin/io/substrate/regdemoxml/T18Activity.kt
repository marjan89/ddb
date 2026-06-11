package io.substrate.regdemoxml

import android.os.Bundle
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T18Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t18)
        val content = findViewById<TextView>(R.id.t18_content)
        fun setTab(label: String) {
            content.text = label
            content.contentDescription = label
        }
        setTab("T18 Tab Alpha Content")
        findViewById<Button>(R.id.t18_goto_alpha).setOnClickListener { setTab("T18 Tab Alpha Content") }
        findViewById<Button>(R.id.t18_goto_beta).setOnClickListener { setTab("T18 Tab Beta Content") }
        findViewById<Button>(R.id.t18_goto_gamma).setOnClickListener { setTab("T18 Tab Gamma Content") }
    }
}
