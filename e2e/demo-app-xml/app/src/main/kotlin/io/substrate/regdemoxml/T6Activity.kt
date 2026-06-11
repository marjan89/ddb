package io.substrate.regdemo.xml

import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T6Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t6)
        val input = findViewById<EditText>(R.id.t6_password)
        val mirror = findViewById<TextView>(R.id.t6_mirror)
        input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                mirror.text = s?.toString().orEmpty()
            }
            override fun afterTextChanged(s: Editable?) {}
        })
    }
}
