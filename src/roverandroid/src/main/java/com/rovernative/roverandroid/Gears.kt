package com.rovernative.roverandroid

import android.app.Activity
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

    external fun start(context: Context)


    fun createView(context: Activity): View {
        val containerView = RelativeLayout(context)

        return containerView
    }

    fun createTextView(context: Activity): TextView {

        val textView = TextView(context)

        textView.text = "Rover"

        textView.textSize = 20f
        textView.gravity = Gravity.CENTER

        return textView
    }
}