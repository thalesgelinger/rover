package com.rovernative.roverandroid

import android.content.Context
import android.util.Log
import java.io.File
import java.io.IOException

object FileUtils {
    fun createFolderIfNotExists(context: Context, folderName: String): File {
        val directory = File(context.filesDir, folderName)

        if (!directory.exists()) {
            if (directory.mkdirs()) {
                Log.i("Folder Creation", "Directory created successfully")
            } else {
                Log.e("Folder Creation", "Failed to create directory")
            }
        } else {
            Log.i("Folder Creation", "Directory already exists")
        }

        return directory
    }

    fun writeFile(context: Context, path: String, content: String): String {
        val fullPath = File(context.filesDir, path )
        val parentDir = fullPath.parentFile

        if (!parentDir.exists() && !parentDir.mkdirs()) {
            throw IOException("Failed to create directory: ${parentDir.absolutePath}")
        }

        fullPath.writeText(content)

        return fullPath.absolutePath
    }}