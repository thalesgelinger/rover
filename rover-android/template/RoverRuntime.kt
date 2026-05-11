package lu.rover.host

object RoverRuntime {
    init {
        System.loadLibrary("rover_android")
    }

    fun init(host: MainActivity): Long = nativeInit(host)
    fun free(runtime: Long) = nativeFree(runtime)
    fun loadLua(runtime: Long, source: String): Int = nativeLoadLua(runtime, source)
    fun tick(runtime: Long): Int = nativeTick(runtime)
    fun nextWakeMs(runtime: Long): Int = nativeNextWakeMs(runtime)
    fun dispatchClick(runtime: Long, id: Int): Int = nativeDispatchClick(runtime, id)
    fun dispatchInput(runtime: Long, id: Int, value: String): Int = nativeDispatchInput(runtime, id, value)
    fun dispatchSubmit(runtime: Long, id: Int, value: String): Int = nativeDispatchSubmit(runtime, id, value)
    fun dispatchToggle(runtime: Long, id: Int, checked: Boolean): Int = nativeDispatchToggle(runtime, id, checked)
    fun setViewport(runtime: Long, width: Int, height: Int): Int = nativeSetViewport(runtime, width, height)
    fun lastError(runtime: Long): String = nativeLastError(runtime)

    private external fun nativeInit(host: MainActivity): Long
    private external fun nativeFree(runtime: Long)
    private external fun nativeLoadLua(runtime: Long, source: String): Int
    private external fun nativeTick(runtime: Long): Int
    private external fun nativeNextWakeMs(runtime: Long): Int
    private external fun nativeDispatchClick(runtime: Long, id: Int): Int
    private external fun nativeDispatchInput(runtime: Long, id: Int, value: String): Int
    private external fun nativeDispatchSubmit(runtime: Long, id: Int, value: String): Int
    private external fun nativeDispatchToggle(runtime: Long, id: Int, checked: Boolean): Int
    private external fun nativeSetViewport(runtime: Long, width: Int, height: Int): Int
    private external fun nativeLastError(runtime: Long): String
}
