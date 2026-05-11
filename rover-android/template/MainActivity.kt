package lu.rover.host

import android.app.Activity
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.TextWatcher
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.CheckBox
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import java.nio.charset.StandardCharsets
import kotlin.math.max

class MainActivity : Activity() {
    private val handler = Handler(Looper.getMainLooper())
    private val views = mutableMapOf<Long, View>()
    private val styles = mutableMapOf<Long, StyleState>()
    private lateinit var root: FrameLayout
    private var runtime: Long = 0
    private var ticking = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        root = FrameLayout(this)
        setContentView(root)

        runtime = RoverRuntime.init(this)
        val source = assets.open("bundle.lua").use { input ->
            String(input.readBytes(), StandardCharsets.UTF_8)
        }
        checkResult(RoverRuntime.loadLua(runtime, source))
        root.post {
            RoverRuntime.setViewport(runtime, pxToDp(root.width), pxToDp(root.height))
            tickAndSchedule()
        }
    }

    override fun onDestroy() {
        if (runtime != 0L) {
            RoverRuntime.free(runtime)
            runtime = 0
        }
        super.onDestroy()
    }

    fun createView(nodeId: Long, kind: Int): Long {
        val view = when (kind) {
            KIND_ROOT -> root
            KIND_ROW -> LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            KIND_COLUMN, KIND_VIEW -> LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            KIND_TEXT -> TextView(this)
            KIND_BUTTON -> Button(this).apply {
                setOnClickListener {
                    RoverRuntime.dispatchClick(runtime, nodeId.toInt())
                    tickAndSchedule()
                }
            }
            KIND_INPUT -> EditText(this).apply {
                addTextChangedListener(object : TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
                    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                        RoverRuntime.dispatchInput(runtime, nodeId.toInt(), s?.toString().orEmpty())
                        scheduleTick()
                    }
                    override fun afterTextChanged(s: Editable?) = Unit
                })
                setOnEditorActionListener { v, _, _ ->
                    RoverRuntime.dispatchSubmit(runtime, nodeId.toInt(), v.text?.toString().orEmpty())
                    tickAndSchedule()
                    false
                }
            }
            KIND_CHECKBOX -> CheckBox(this).apply {
                setOnCheckedChangeListener { _, checked ->
                    RoverRuntime.dispatchToggle(runtime, nodeId.toInt(), checked)
                    tickAndSchedule()
                }
            }
            KIND_IMAGE -> ImageView(this)
            KIND_SCROLL_VIEW -> ScrollView(this)
            else -> LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        }
        views[nodeId] = view
        return nodeId
    }

    fun appendChild(parentHandle: Long, childHandle: Long) {
        val parent = views[parentHandle] as? ViewGroup ?: return
        val child = views[childHandle] ?: return
        if (child.parent === parent) return
        (child.parent as? ViewGroup)?.removeView(child)
        parent.addView(child, defaultLayoutParams(parent))
    }

    fun removeView(handle: Long) {
        val view = views.remove(handle) ?: return
        (view.parent as? ViewGroup)?.removeView(view)
    }

    fun setText(handle: Long, value: String) {
        when (val view = views[handle]) {
            is TextView -> if (view.text.toString() != value) view.text = value
            is Button -> if (view.text.toString() != value) view.text = value
            is EditText -> if (view.text.toString() != value) view.setText(value)
            is ImageView -> view.contentDescription = value
        }
    }

    fun setBool(handle: Long, value: Boolean) {
        val checkbox = views[handle] as? CheckBox ?: return
        if (checkbox.isChecked != value) checkbox.isChecked = value
    }

    fun setFrame(handle: Long, x: Float, y: Float, width: Float, height: Float) {
        val view = views[handle] ?: return
        val parent = view.parent as? ViewGroup
        val params = FrameLayout.LayoutParams(max(1, dp(width)), max(1, dp(height))).apply {
            leftMargin = dp(x)
            topMargin = dp(y)
        }
        if (parent is LinearLayout) {
            view.layoutParams = LinearLayout.LayoutParams(max(1, dp(width)), max(1, dp(height)))
        } else {
            view.layoutParams = params
        }
    }

    fun setStyle(handle: Long, flags: Int, bgRgba: Int, borderRgba: Int, textRgba: Int, borderWidth: Int) {
        val view = views[handle] ?: return
        val style = styles.getOrPut(handle) { StyleState() }
        style.flags = flags
        style.bgRgba = bgRgba
        style.borderRgba = borderRgba
        style.textRgba = textRgba
        style.borderWidth = borderWidth

        if ((flags and HAS_TEXT) != 0 && view is TextView) {
            view.setTextColor(rgbaToArgb(textRgba))
        }
        if ((flags and (HAS_BG or HAS_BORDER or HAS_BORDER_WIDTH)) != 0) {
            view.background = GradientDrawable().apply {
                if ((flags and HAS_BG) != 0) setColor(rgbaToArgb(bgRgba))
                if ((flags and HAS_BORDER) != 0 || (flags and HAS_BORDER_WIDTH) != 0) {
                    setStroke(dp(borderWidth.toFloat()), rgbaToArgb(borderRgba))
                }
            }
        }
    }

    private fun tickAndSchedule() {
        if (runtime == 0L) return
        checkResult(RoverRuntime.tick(runtime))
        scheduleTick()
    }

    private fun scheduleTick() {
        if (runtime == 0L || ticking) return
        val nextWake = RoverRuntime.nextWakeMs(runtime)
        if (nextWake < 0) return
        ticking = true
        handler.postDelayed({
            ticking = false
            tickAndSchedule()
        }, nextWake.toLong())
    }

    private fun checkResult(code: Int) {
        if (code != 0) error(RoverRuntime.lastError(runtime))
    }

    private fun defaultLayoutParams(parent: ViewGroup): ViewGroup.LayoutParams = when (parent) {
        is LinearLayout -> LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        else -> FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)
    }

    private fun dp(value: Float): Int = (value * resources.displayMetrics.density).toInt()
    private fun pxToDp(value: Int): Int = (value / resources.displayMetrics.density).toInt()

    private fun rgbaToArgb(rgba: Int): Int {
        val r = (rgba ushr 24) and 0xff
        val g = (rgba ushr 16) and 0xff
        val b = (rgba ushr 8) and 0xff
        val a = rgba and 0xff
        return Color.argb(a, r, g, b)
    }

    private data class StyleState(
        var flags: Int = 0,
        var bgRgba: Int = 0,
        var borderRgba: Int = 0,
        var textRgba: Int = 0,
        var borderWidth: Int = 0,
    )

    companion object {
        private const val KIND_ROOT = 0
        private const val KIND_VIEW = 1
        private const val KIND_COLUMN = 2
        private const val KIND_ROW = 3
        private const val KIND_TEXT = 4
        private const val KIND_BUTTON = 5
        private const val KIND_INPUT = 6
        private const val KIND_CHECKBOX = 7
        private const val KIND_IMAGE = 8
        private const val KIND_SCROLL_VIEW = 9

        private const val HAS_BG = 1 shl 0
        private const val HAS_BORDER = 1 shl 1
        private const val HAS_TEXT = 1 shl 2
        private const val HAS_BORDER_WIDTH = 1 shl 3
    }
}
