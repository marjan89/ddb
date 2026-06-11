package io.substrate.regdemo.xml

import android.content.Context
import android.os.Bundle
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T5Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t5)
        val input = findViewById<EditText>(R.id.t5_input)
        val status = findViewById<TextView>(R.id.t5_status)
        input.setOnFocusChangeListener { _, has -> status.text = if (has) "shown" else "hidden" }
        findViewById<Button>(R.id.t5_dismiss).setOnClickListener {
            val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
            imm.hideSoftInputFromWindow(input.windowToken, 0)
            input.clearFocus()
            status.text = "hidden"
        }
    }
}
