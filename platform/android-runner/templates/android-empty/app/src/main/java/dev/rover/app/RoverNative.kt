package dev.rover.app

object RoverNative {
    external fun initVulkan(entryPath: String, surface: android.view.Surface): Long
    external fun renderVulkan(handle: Long): Boolean
    external fun pointerTap(handle: Long, x: Float, y: Float): Boolean
    external fun destroyVulkan(handle: Long)

    init {
        try {
            System.loadLibrary("rover_runtime")
        } catch (_: UnsatisfiedLinkError) {
        }
    }
}
