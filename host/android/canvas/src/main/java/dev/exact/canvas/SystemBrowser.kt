package dev.exact.canvas

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle

/** The Custom Tabs intent protocol, also used by @exact/in-app-browser's AndroidX builder.
 * A null SESSION binder requests a tab without binding a service.
 * https://developer.android.com/reference/androidx/browser/customtabs/CustomTabsIntent#EXTRA_SESSION */
object SystemBrowser {
    const val SERVICE = "android.support.customtabs.action.CustomTabsService"
    const val SESSION = "android.support.customtabs.extra.SESSION"

    /** [intent] may carry a package's AndroidX options. Auth selects only browsers so a claimed
     * authorization link cannot open another app; ordinary content keeps Android's link resolution. */
    fun launch(activity: Activity, uri: Uri, intent: Intent = Intent(Intent.ACTION_VIEW), browserOnly: Boolean = false, options: Bundle? = null) {
        intent.data = uri
        if (!browserOnly) {
            if (!intent.hasExtra(SESSION)) intent.putExtras(Bundle().apply { putBinder(SESSION, null) })
            activity.startActivity(intent, options)
            return
        }
        val manager = activity.packageManager
        val probe = Intent(Intent.ACTION_VIEW, Uri.parse("https://"))
        fun handlers(scheme: String) = manager.queryIntentActivities(
            Intent(Intent.ACTION_VIEW, Uri.parse("$scheme://")),
            PackageManager.MATCH_DEFAULT_ONLY or PackageManager.GET_RESOLVED_FILTER,
        ).filter { info ->
            // An app claiming the authorization host must not receive a browser session.
            val filter = info.filter
            filter != null && filter.hasCategory(Intent.CATEGORY_BROWSABLE) &&
                filter.hasDataScheme(scheme) && filter.countDataAuthorities() == 0
        }.map { it.activityInfo.packageName }.toSet()
        val browsers = handlers("https").intersect(handlers("http")).toMutableList()
        fun target(pkg: String) = Intent(intent).setPackage(pkg).apply {
            if (manager.resolveService(Intent(SERVICE).setPackage(pkg), 0) != null) {
                if (!hasExtra(SESSION)) putExtras(Bundle().apply { putBinder(SESSION, null) })
            } else removeExtra(SESSION)
        }
        val default = manager.resolveActivity(probe, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo?.packageName
        if (default in browsers) {
            try {
                activity.startActivity(target(default!!), options)
                return
            } catch (_: ActivityNotFoundException) {
                // A browser can disappear between resolution and launch. Keep the remaining choices.
                browsers.remove(default)
            }
        }
        val choices = browsers.map(::target)
        if (choices.isEmpty()) throw ActivityNotFoundException("no system browser")
        val launch = if (choices.size == 1) choices.first() else {
            Intent.createChooser(choices.first(), null)
                .putExtra(Intent.EXTRA_INITIAL_INTENTS, choices.drop(1).toTypedArray())
        }
        activity.startActivity(launch, options)
    }
}
