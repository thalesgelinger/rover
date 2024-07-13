package com.rovernative.roverandroid

import android.content.Context
import android.util.Log
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.RelativeLayout
import android.widget.TextView

object Gears {
    init {
        try {
            System.loadLibrary("gears")
            Log.d("JNI", "Library loaded successfully")
        } catch (e: UnsatisfiedLinkError) {
            Log.e("JNI", "Failed to load library", e)
        }
    }

    external fun start(context: Context): View

    external fun greeting(string: String): String

    fun createView(context: Context): RelativeLayout {
        val containerView = RelativeLayout(context)

        val textView = createTextView(context)

        containerView.addView(textView)

        val layoutParams = RelativeLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT
        )
        layoutParams.addRule(RelativeLayout.CENTER_IN_PARENT, RelativeLayout.TRUE)
        textView.layoutParams = layoutParams
        return containerView
    }

    fun createTextView(context: Context): TextView {

        val textView = TextView(context)

        textView.text = greeting("Rover")

        textView.textSize = 20f
        textView.gravity = Gravity.CENTER

        val layoutParams = ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT
        )
        textView.layoutParams = layoutParams

        return textView
    }
}