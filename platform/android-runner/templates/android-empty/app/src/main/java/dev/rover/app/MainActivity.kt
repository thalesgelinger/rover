package dev.rover.app

import android.os.Bundle
import android.util.Log
import android.view.Choreographer
import android.view.Gravity
import android.view.MotionEvent
import android.widget.FrameLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity(), RoverSurfaceView.Listener {
    private var nativeHandle: Long = 0L
    private lateinit var surfaceView: RoverSurfaceView
    private lateinit var root: FrameLayout
    private var banner: TextView? = null
    private var entryPath: String = ""
    private val choreographer: Choreographer by lazy { Choreographer.getInstance() }
    private var isRendering: Boolean = false
    private lateinit var frameCallback: Choreographer.FrameCallback
    private var lastRenderOk: Boolean = true

    init {
        frameCallback = Choreographer.FrameCallback {
            if (isRendering && nativeHandle != 0L) {
                val reloading = RoverNative.isReloading(nativeHandle)
                if (reloading && banner == null) showBanner()
                else if (!reloading && banner != null) hideBanner()
                
                val ok = RoverNative.renderVulkan(nativeHandle)
                if (!ok && lastRenderOk) {
                    Log.e("Rover", "renderVulkan returned false")
                }
                lastRenderOk = ok
                choreographer.postFrameCallback(frameCallback)
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        entryPath = copyAssetsToFiles()
        Log.i("Rover", "entryPath=$entryPath")

        surfaceView = RoverSurfaceView(this)
        surfaceView.listener = this
        surfaceView.setOnTouchListener { _, event ->
            if (event.action == MotionEvent.ACTION_UP && nativeHandle != 0L) {
                RoverNative.pointerTap(nativeHandle, event.x, event.y)
            }
            true
        }
        root = FrameLayout(this)
        root.addView(surfaceView)
        setContentView(root)
    }

    override fun onResume() {
        super.onResume()
        if (nativeHandle != 0L) {
            startRendering()
        }
    }

    override fun onPause() {
        super.onPause()
        stopRendering()
    }

    override fun onSurfaceReady(surface: android.view.Surface) {
        if (nativeHandle != 0L) return
        nativeHandle = RoverNative.initVulkan(entryPath, surface, resources.displayMetrics.density)
        Log.i("Rover", "initVulkan handle=$nativeHandle")
        if (nativeHandle != 0L) {
            RoverNative.enableHotReload(nativeHandle)
            startRendering()
        } else {
            Log.e("Rover", "initVulkan failed")
        }
    }

    override fun onSurfaceChanged(surface: android.view.Surface, width: Int, height: Int) {
        if (nativeHandle != 0L) {
            RoverNative.surfaceChanged(nativeHandle, width, height)
            startRendering()
        }
    }

    override fun onSurfaceDestroyed() {
        stopRendering()
        if (nativeHandle != 0L) {
            RoverNative.destroyVulkan(nativeHandle)
            nativeHandle = 0
        }
    }

    private fun startRendering() {
        if (isRendering || nativeHandle == 0L) return
        isRendering = true
        choreographer.postFrameCallback(frameCallback)
    }

    private fun stopRendering() {
        if (!isRendering) return
        isRendering = false
        choreographer.removeFrameCallback(frameCallback)
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
        return outDir.absolutePath
    }

    private fun showBanner() {
        if (banner != null) return
        val tv = TextView(this)
        tv.text = "RELOADING..."
        tv.setBackgroundColor(0xE6FF9800.toInt())
        tv.setTextColor(0xFFFFFFFF.toInt())
        tv.textSize = 12f
        tv.setPadding(16, 8, 16, 8)
        val params = FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT)
        params.gravity = Gravity.TOP or Gravity.START
        params.leftMargin = 16
        params.topMargin = 16
        banner = tv
        root.addView(tv, params)
    }
    
    private fun hideBanner() {
        banner?.let { root.removeView(it) }
        banner = null
    }
}
