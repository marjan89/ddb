package io.substrate.regdemoxml

import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T12Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t12)
        val input = findViewById<EditText>(R.id.t12_input)
        val mirror = findViewById<TextView>(R.id.t12_value)
        mirror.text = "T12 Value: "
        mirror.contentDescription = "T12 Value: "
        input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                val v = s?.toString().orEmpty()
                mirror.text = "T12 Value: $v"
                mirror.contentDescription = "T12 Value: $v"
                // Mirror raw value too so element_exists fuzzy match finds the value
                // directly (matches t12.yaml asserting "T12 pasted content").
            }
            override fun afterTextChanged(s: Editable?) {}
        })
    }
}
