package io.substrate.regdemoxml

import android.os.Bundle
import android.view.animation.LinearInterpolator
import android.widget.ImageView
import androidx.appcompat.app.AppCompatActivity

class T17Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t17)
        val spinner = findViewById<ImageView>(R.id.t17_spinner)
        spinner.animate()
            .rotationBy(360f)
            .setDuration(1000)
            .setInterpolator(LinearInterpolator())
            .withEndAction(object : Runnable {
                override fun run() {
                    spinner.animate()
                        .rotationBy(360f)
                        .setDuration(1000)
                        .setInterpolator(LinearInterpolator())
                        .withEndAction(this)
                        .start()
                }
            })
            .start()
    }
}
