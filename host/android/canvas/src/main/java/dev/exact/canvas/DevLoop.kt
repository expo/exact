package dev.exact.canvas

import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.view.Gravity
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.FrameLayout
import android.widget.TextView
import kotlin.math.roundToInt

internal object DevLoop {
    private fun url(value: String?): String? = value?.takeIf {
        val uri = Uri.parse(it)
        (uri.scheme == "http" || uri.scheme == "https") && !uri.host.isNullOrEmpty()
    }

    /** Only a launch through the shell-permission alias can change what the next process follows. */
    fun atLaunch(app: Context, intent: Intent?): String? {
        val extra = intent?.getStringExtra("x_dev_plan")
        val select = intent?.component?.className == "dev.exact.app.DevLoop"
        if (!select) {
            if (extra != null) return extra
            if (intent?.hasExtra("x_plan") == true || intent?.hasExtra("x_agent") == true) return null
        }
        val prefs = app.getSharedPreferences("exact.devLoop", Context.MODE_PRIVATE)
        val installed = app.packageManager.getPackageInfo(app.packageName, 0).lastUpdateTime
        if (select) {
            val requested = url(extra)
            prefs.edit().putLong("installed", installed).putString("url", requested).apply()
            return requested
        }
        return if (prefs.getLong("installed", -1) == installed) prefs.getString("url", null) else null
    }

    class Notice(context: Context, root: FrameLayout, url: String) {
        private val density = context.resources.displayMetrics.density
        private val address = Uri.parse(url).let { uri ->
            (uri.host ?: "") + if (uri.port >= 0) ":${uri.port}" else ""
        }
        private val margin = dp(4)
        val label = TextView(context).apply {
            textSize = 12f
            setTextColor(Color.WHITE)
            setPadding(dp(8), dp(3), dp(8), dp(3))
            background = GradientDrawable().apply { cornerRadius = dp(5).toFloat(); setColor(0xe6333333.toInt()) }
            visibility = android.view.View.GONE
        }
        init { root.addView(label, FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT, Gravity.TOP or Gravity.RIGHT)) }
        private fun dp(value: Int) = (value * density).roundToInt()
        fun show(status: Int) {
            val message = when (status) {
                1 -> "Connecting to $address"
                2 -> "Following $address"
                3 -> "Dev loop unavailable · edits paused"
                4 -> "Waiting for dev build"
                5 -> "Dev build failed · edits paused"
                else -> null
            }
            label.visibility = if (message == null) android.view.View.GONE else android.view.View.VISIBLE
            if (message != null && label.text != message) label.text = message
        }
        fun place(top: Int, right: Int) {
            val lp = label.layoutParams as FrameLayout.LayoutParams
            if (lp.rightMargin != right + margin || lp.topMargin != top + margin) {
                lp.rightMargin = right + margin
                lp.topMargin = top + margin
                label.layoutParams = lp
            }
        }
    }
}
