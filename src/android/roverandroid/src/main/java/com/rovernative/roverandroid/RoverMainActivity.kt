package com.rovernative.roverandroid

import android.content.Context
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.RelativeLayout
import android.widget.TextView
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity


open class RoverMainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        val containerView = RelativeLayout(this)

        val textView = createTextView(this)

        containerView.addView(textView)

        val layoutParams = RelativeLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT
        )
        layoutParams.addRule(RelativeLayout.CENTER_IN_PARENT, RelativeLayout.TRUE)
        textView.layoutParams = layoutParams

        setContentView(containerView)
    }
}


fun createTextView(context: Context): TextView {

    val textView = TextView(context)

    textView.text = Gears.greeting("Rover Android")

    textView.textSize = 20f
    textView.gravity = Gravity.CENTER

    val layoutParams = ViewGroup.LayoutParams(
        ViewGroup.LayoutParams.WRAP_CONTENT,
        ViewGroup.LayoutParams.WRAP_CONTENT
    )
    textView.layoutParams = layoutParams

    return textView
}