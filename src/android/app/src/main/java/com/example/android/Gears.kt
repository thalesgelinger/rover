package com.example.android

object Gears {
    init {
        System.loadLibrary("gears")
    }

    external fun greeting(name: String): String
}
