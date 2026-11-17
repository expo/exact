package dev.exact.widgets

import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle

/**
 * One widget kind's provider (LLP 1126.003 D5). The app's build makes a subclass per kind, since Android names a
 * provider by its class, and names its kind in the receiver's `dev.exact.widget.kind` meta-data; the receivers run in
 * the app's `:widgets` process. Every broadcast only enqueues work on [Widgets]' worker.
 */
open class Provider : AppWidgetProvider() {
    private fun kind(context: Context): String? = kindOf(context, javaClass.name)

    override fun onReceive(context: Context, intent: Intent) {
        if (Build.VERSION.SDK_INT < 31) return
        val kind = kind(context) ?: return
        when (intent.action) {
            ACTION_TICK -> Widgets.request(context, kind, false, null, goAsync())
            Intent.ACTION_LOCALE_CHANGED, Intent.ACTION_TIME_CHANGED, Intent.ACTION_TIMEZONE_CHANGED ->
                Widgets.request(context, kind, true, null, goAsync())
            else -> super.onReceive(context, intent)
        }
    }

    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        kind(context)?.let { Widgets.request(context, it, true, ids, goAsync()) }
    }

    /** A resize renders from the kept timeline, with no refresh (D4). */
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) {
        kind(context)?.let { Widgets.request(context, it, false, intArrayOf(id), goAsync()) }
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        ids.forEach { Store.forget(context, it) }
    }

    override fun onRestored(context: Context, old: IntArray, new: IntArray) {
        old.zip(new).forEach { (from, to) -> Store.move(context, from, to) }
    }

    companion object {
        /** The provider's own alarm (D5): render, and refresh when due. */
        const val ACTION_TICK = "dev.exact.widgets.TICK"

        internal fun kindOf(context: Context, provider: String): String? = runCatching {
            context.packageManager.getReceiverInfo(ComponentName(context, provider), PackageManager.GET_META_DATA)
                .metaData?.getString("dev.exact.widget.kind")
        }.getOrNull()
    }
}
