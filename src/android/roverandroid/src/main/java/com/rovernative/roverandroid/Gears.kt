package com.rovernative.roverandroid

object Gears {
    init {
        System.loadLibrary("gears")
    }

    external fun greeting(name: String): String
}
