package io.substrate.regdemoxml

import android.app.AlertDialog
import android.os.Bundle
import android.widget.Button
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T7Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t7)
        val status = findViewById<TextView>(R.id.t7_status)
        status.text = "closed"
        findViewById<Button>(R.id.t7_show_alert).setOnClickListener {
            status.text = "open"
            AlertDialog.Builder(this)
                .setTitle("Alert")
                .setPositiveButton("OK") { d, _ ->
                    d.dismiss()
                    status.text = "closed"
                }
                .setOnCancelListener { status.text = "closed" }
                .show()
        }
    }
}
