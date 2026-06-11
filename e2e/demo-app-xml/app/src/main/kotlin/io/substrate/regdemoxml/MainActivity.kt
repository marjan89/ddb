package io.substrate.regdemo.xml

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {

    private val routes: List<Triple<Int, String, Class<*>>> = listOf(
        Triple(1,  "T1 Launch",       T1Activity::class.java),
        Triple(2,  "T2 Type",         T2Activity::class.java),
        Triple(3,  "T3 Tap",          T3Activity::class.java),
        Triple(4,  "T4 Navigate",     T4Activity::class.java),
        Triple(5,  "T5 Kbd Dismiss",  T5Activity::class.java),
        Triple(6,  "T6 Secure Field", T6Activity::class.java),
        Triple(7,  "T7 Dialog",       T7Activity::class.java),
        Triple(8,  "T8 Scroll",       T8Activity::class.java),
        Triple(9,  "T9 Wait",         T9Activity::class.java),
        Triple(10, "T10 Mock URL",    T10Activity::class.java),
        Triple(11, "T11 Deep Nav",    T11Activity::class.java),
        Triple(12, "T12 Paste",       T12Activity::class.java),
        Triple(13, "T13 Login",       T13Activity::class.java),
        Triple(14, "T14 Screen",      T14Activity::class.java),
        Triple(15, "T15 State",       T15Activity::class.java),
        Triple(16, "T16 Text Equal",  T16Activity::class.java),
        Triple(17, "T17 Animations",  T17Activity::class.java),
        Triple(18, "T18 Tabs",        T18Activity::class.java),
        Triple(19, "T19 Sheet",       T19Activity::class.java),
        Triple(20, "T20 Refresh",     T20Activity::class.java),
        Triple(21, "T21 Press Back",  T21Activity::class.java),
        Triple(22, "T22 Long Press",  T22Activity::class.java),
        Triple(23, "T23 Capture",     T23Activity::class.java),
        Triple(24, "T24 Screenshot",  T24Activity::class.java),
        Triple(25, "T25 Scroll",      T25Activity::class.java),
        Triple(26, "T26 Hide Show",   T26Activity::class.java),
    )

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        routes.forEach { (n, _, cls) ->
            val resId = resources.getIdentifier("t${n}_launch", "id", packageName)
            findViewById<Button>(resId)?.setOnClickListener {
                startActivity(Intent(this, cls))
            }
        }
    }
}
