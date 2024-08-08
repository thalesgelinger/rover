package com.rovernative.roverandroid

import android.content.Context
import android.util.Log
import android.util.TypedValue
import com.google.gson.Gson
import com.google.gson.GsonBuilder
import com.google.gson.reflect.TypeToken
import java.io.File

fun parseProps(propsStr: String): ViewProps{

    val gson = GsonBuilder()
        .registerTypeAdapter(Size::class.java, SizeDeserializer())
        .create()

    val viewProps = gson.fromJson(propsStr, ViewProps::class.java)
    return viewProps
}

fun dpToPx(context: Context, dp: Int): Int {
    return TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, dp.toFloat(), context.resources.displayMetrics).toInt()
}


fun getFullPath(context: Context, relativePath: String): String {
    val baseDir = context.filesDir // or context.getExternalFilesDir(null) for external storage

    val fullPath = File(baseDir, relativePath)

    return fullPath.absolutePath
}