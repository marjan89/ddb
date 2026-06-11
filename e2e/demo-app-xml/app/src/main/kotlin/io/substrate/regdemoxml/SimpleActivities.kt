package io.substrate.regdemo.xml

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity

// Simple "set content view" activities for screens with no behavior beyond
// rendering static text. Each picks its layout via R.layout.activity_t<N>.
open class StaticActivity(private val layoutResId: Int) : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(layoutResId)
    }
}

class T1Activity : StaticActivity(R.layout.activity_t1)
class T16Activity : StaticActivity(R.layout.activity_t16)
class T21Activity : StaticActivity(R.layout.activity_t21)
class T23Activity : StaticActivity(R.layout.activity_t23)
class T24Activity : StaticActivity(R.layout.activity_t24)
