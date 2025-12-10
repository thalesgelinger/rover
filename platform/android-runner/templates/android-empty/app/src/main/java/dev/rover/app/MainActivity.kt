package dev.rover.app

import android.os.Bundle
import android.os.Bundle
import android.view.Choreographer
import android.view.MotionEvent
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity(), RoverSurfaceView.Listener {
    private var nativeHandle: Long = 0
    private lateinit var surfaceView: RoverSurfaceView
    private var entryPath: String = ""

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        entryPath = copyAssetsToFiles()

        surfaceView = RoverSurfaceView(this)
        surfaceView.listener = this
        surfaceView.setOnTouchListener { _, event ->
            if (event.action == MotionEvent.ACTION_UP && nativeHandle != 0L) {
                RoverNative.pointerTap(nativeHandle, event.x, event.y)
            }
            true
        }
        setContentView(surfaceView)
    }

    override fun onSurfaceReady(surface: android.view.Surface) {
        if (nativeHandle != 0L) return
        nativeHandle = RoverNative.initVulkan(entryPath, surface)
        if (nativeHandle != 0L) {
            scheduleFrame()
        }
    }

    override fun onSurfaceDestroyed() {
        if (nativeHandle != 0L) {
            RoverNative.destroyVulkan(nativeHandle)
            nativeHandle = 0
        }
    }

    private fun scheduleFrame() {
        Choreographer.getInstance().postFrameCallback { _ ->
            if (nativeHandle != 0L) {
                RoverNative.renderVulkan(nativeHandle)
                scheduleFrame()
            }
        }
    }

    private fun copyAssetsToFiles(): String {
        val outDir = filesDir.resolve("rover")
        if (outDir.exists()) outDir.deleteRecursively()
        outDir.mkdirs()
        assets.list("rover")?.forEach { name ->
            assets.open("rover/$name").use { input ->
                outDir.resolve(name).outputStream().use { output ->
                    input.copyTo(output)
                }
            }
        }
        return outDir.resolve("main.lua").absolutePath
    }
}
