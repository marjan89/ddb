package io.substrate.regdemo.xml

import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T2Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t2)
        val input = findViewById<EditText>(R.id.t2_input)
        val mirror = findViewById<TextView>(R.id.t2_mirror)
        input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                mirror.text = s?.toString().orEmpty()
            }
            override fun afterTextChanged(s: Editable?) {}
        })
    }
}
