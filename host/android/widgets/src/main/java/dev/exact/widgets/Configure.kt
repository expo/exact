package dev.exact.widgets

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ScrollView
import android.widget.TextView
import org.json.JSONObject

/**
 * A placement's configuration (LLP 1126.003 D7): each of the row's parameters with its titled choices, in the device's
 * theme. A choice is kept for this placement and renders it at once; leaving changes nothing. Placing the widget uses
 * the defaults (`configuration_optional`), and the launcher's "Customise" opens this again (`reconfigurable`).
 */
class Configure : Activity() {
    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        setResult(RESULT_CANCELED, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
        val info = AppWidgetManager.getInstance(this).getAppWidgetInfo(id)
        val kind = info?.provider?.className?.let { Provider.kindOf(this, it) }
        val row = kind?.let { Row.of(this, it) }
        if (row == null || row.parameters.isEmpty()) return finish()
        title = row.title
        val chosen = Store.configuration(this, id)?.let { JSONObject(it) } ?: row.defaults()
        val pad = (16 * resources.displayMetrics.density).toInt()
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad, pad, pad) }
        for (parameter in row.parameters) {
            column.addView(TextView(this).apply {
                text = parameter.title
                setTextAppearance(android.R.style.TextAppearance_DeviceDefault_Medium)
                isAccessibilityHeading = true
            })
            val group = RadioGroup(this)
            for ((value, label) in parameter.values) {
                val button = RadioButton(this).apply { text = label; this.id = android.view.View.generateViewId() }
                group.addView(button, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
                if (chosen.optString(parameter.name, parameter.default) == value) group.check(button.id)
                button.setOnClickListener {
                    chosen.put(parameter.name, value)
                    Store.configure(this, id, chosen)
                    setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
                    if (android.os.Build.VERSION.SDK_INT >= 31) Widgets.request(this, kind, false, intArrayOf(id), null)
                }
            }
            column.addView(group)
        }
        // Edge to edge (targetSdk 35), the content starts under the action bar and the status bar: it takes their insets.
        setContentView(ScrollView(this).apply { fitsSystemWindows = true; addView(column) })
    }
}
