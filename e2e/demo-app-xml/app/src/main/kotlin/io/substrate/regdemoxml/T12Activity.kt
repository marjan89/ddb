package io.substrate.regdemo.xml

import android.content.Context
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class T12Activity : AppCompatActivity() {
    private lateinit var input: EditText

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t12)
        input = findViewById(R.id.t12_input)
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

    override fun onResume() {
        super.onResume()
        // TD-131: agent's /text-field/set + /text-field/paste expect a
        // FOCUSED EditText. AppCompat XML EditText doesn't auto-focus on
        // Activity resume (unlike Compose TextField in the sibling demo,
        // which gets focus from the navigation stack). Drive focus + show
        // IME explicitly so the agent endpoints find an EditText to write
        // into. Matches Compose demo's effective behavior.
        input.requestFocus()
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imm.showSoftInput(input, InputMethodManager.SHOW_IMPLICIT)
    }
}
