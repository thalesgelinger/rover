package dev.rover.app

object RoverNative {
    external fun initVulkan(entryPath: String, surface: android.view.Surface, scale: Float): Long
    external fun renderVulkan(handle: Long): Boolean
    external fun surfaceChanged(handle: Long, width: Int, height: Int)
    external fun pointerTap(handle: Long, x: Float, y: Float): Boolean
    external fun destroyVulkan(handle: Long)
    external fun enableHotReload(handle: Long): Boolean

    init {
        try {
            System.loadLibrary("rover_runtime")
        } catch (_: UnsatisfiedLinkError) {
        }
    }
}
