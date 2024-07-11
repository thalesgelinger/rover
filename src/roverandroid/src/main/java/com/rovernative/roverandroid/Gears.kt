package com.rovernative.roverandroid

import android.util.Log

object Gears {
    init {
        try {
            System.loadLibrary("gears")
            Log.d("JNI", "Library loaded successfully")
        } catch (e: UnsatisfiedLinkError) {
            Log.e("JNI", "Failed to load library", e)
        }
    }

    @JvmStatic
    external fun greeting(name: String): String
}