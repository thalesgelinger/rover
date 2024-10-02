package com.rovernative.roverandroid

import android.content.Context
import android.os.Bundle
import android.util.Log
import android.view.Gravity
import android.view.ViewGroup
import android.widget.RelativeLayout
import android.widget.TextView
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch

open class RoverAndroidActivity: AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        val context = this

        GlobalScope.launch {
            Gears.devServer(context) {
                Log.i("OUT ROVER", it)
                context.runOnUiThread {
                    Gears.start(context, getFullPath(context, it))
                }
            }
        }
    }
}

