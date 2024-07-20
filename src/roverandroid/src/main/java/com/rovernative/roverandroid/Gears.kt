package com.rovernative.roverandroid

import android.app.Activity
import android.content.Context
import android.graphics.Color
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


    fun createView(context: Activity, props: String): View {
        val viewProps = parseProps(props)

        val containerView = RelativeLayout(context)

        val width = when (viewProps.width) {
            is Size.Value -> dpToPx(context, viewProps.width.size)
            is Size.Full -> ViewGroup.LayoutParams.MATCH_PARENT
            else -> ViewGroup.LayoutParams.WRAP_CONTENT
        }

        val height = when (viewProps.height) {
            is Size.Value -> dpToPx(context, viewProps.height.size)
            is Size.Full -> ViewGroup.LayoutParams.MATCH_PARENT
            else -> ViewGroup.LayoutParams.WRAP_CONTENT
        }

        val layoutParams = RelativeLayout.LayoutParams(width, height)
        containerView.layoutParams = layoutParams

        viewProps.color?.let { colorString ->
            try {
                val color = Color.parseColor(colorString)
                containerView.setBackgroundColor(color)
            } catch (e: IllegalArgumentException) {
                // Handle the case where the color string is invalid
                e.printStackTrace()
            }
        }

        return containerView
    }

    fun createTextView(context: Activity, text: String): TextView {

        val textView = TextView(context)

        textView.text = text

        textView.textSize = 20f
        textView.gravity = Gravity.CENTER

        return textView
    }
}