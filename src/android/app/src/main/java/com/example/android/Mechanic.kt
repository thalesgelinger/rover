package com.example.android

object Mechanic {
    init {
        System.loadLibrary("rover_mechanic")
    }

    external fun greeting(name: String): String
}
