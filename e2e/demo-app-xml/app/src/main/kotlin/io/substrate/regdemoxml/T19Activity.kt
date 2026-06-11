package io.substrate.regdemoxml

import android.os.Bundle
import android.view.View
import android.widget.Button
import androidx.appcompat.app.AppCompatActivity

class T19Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t19)
        val sheet = findViewById<View>(R.id.t19_sheet)
        sheet.visibility = View.GONE
        findViewById<Button>(R.id.t19_show_sheet).setOnClickListener {
            sheet.visibility = View.VISIBLE
        }
        findViewById<Button>(R.id.t19_dismiss_sheet).setOnClickListener {
            sheet.visibility = View.GONE
        }
    }
}
