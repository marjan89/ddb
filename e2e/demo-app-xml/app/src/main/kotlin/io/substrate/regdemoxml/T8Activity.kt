package io.substrate.regdemo.xml

import android.os.Bundle
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T8Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t8)
        val container = findViewById<LinearLayout>(R.id.t8_container)
        // Insert "Row 1..23" between top and bottom markers (rows declared in code for brevity).
        val topIdx = container.indexOfChild(findViewById(R.id.t8_top))
        for (i in 1..23) {
            val tv = TextView(this)
            tv.text = "Row $i"
            tv.contentDescription = "scroll.row.$i"
            val lp = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
            container.addView(tv, topIdx + i, lp)
        }
    }
}
