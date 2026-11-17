package dev.exact.widgets

import android.annotation.TargetApi
import android.app.AlarmManager
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.Parcel
import android.os.SystemClock
import android.util.Log
import android.util.SizeF
import android.widget.RemoteViews
import org.json.JSONObject
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The `:widgets` process's one worker (LLP 1126.003 D5): every refresh and render runs on its thread, in order, so a
 * request that arrives while a kind refreshes is served by another refresh after it, never by the earlier read. A
 * broadcast's work is bounded by [DEADLINE] from its receipt: a timer on the main thread finishes the broadcast then if
 * the worker has not.
 */
@TargetApi(31)
internal object Widgets {
    private const val TAG = "exact-widgets"

    /** A broadcast's whole budget, inside Android's 10 seconds. */
    private const val DEADLINE = 8_000L

    /** A refresh's budget from a warm process (WidgetKit's), and the first after a cold start, which loads the library. */
    private const val WARM = 900L
    private const val COLD = 3_000L

    /** The lead under which a render's alarm is set at its instant rather than halfway ([approach]). */
    private const val LAST_LEAD = 60_000.0

    /** The content margin a side, in dp, which the entry is laid out inside (D3). */
    const val INSET = 16f

    private val thread = HandlerThread("exact-widgets").apply { start() }
    private val worker = Handler(thread.looper)
    private val main = Handler(Looper.getMainLooper())
    private var warm = false

    /** Refresh [kind] (when [refresh]) and render [ids] of it (every placement when null), then finish [pending]. */
    fun request(context: Context, kind: String, refresh: Boolean, ids: IntArray?, pending: BroadcastReceiver.PendingResult?) {
        val app = context.applicationContext
        val done = AtomicBoolean(false)
        val finish = Runnable { if (done.compareAndSet(false, true)) pending?.finish() }
        main.postDelayed(finish, DEADLINE)
        val received = SystemClock.elapsedRealtime()
        worker.post {
            try {
                if (SystemClock.elapsedRealtime() - received < DEADLINE) run(app, kind, refresh, ids)
            } catch (e: Throwable) {
                Log.e(TAG, "widget $kind", e)
            } finally {
                main.removeCallbacks(finish)
                finish.run()
            }
        }
    }

    private fun run(context: Context, kind: String, refresh: Boolean, ids: IntArray?) {
        val store = Store(context, kind)
        val now = System.currentTimeMillis().toDouble()
        var timeline = store.timeline()
        if (refresh || timeline == null || store.due() <= now) {
            val budget = if (warm) WARM else COLD
            warm = true
            val answer = Native.timeline(dir(context), kind.toByteArray(), now, budget).decodeToString()
            timeline = try {
                Timeline.admit(answer).also { store.keep(it, it.refreshDue(now)) }
            } catch (e: IllegalArgumentException) {
                Log.w(TAG, "widget $kind: refresh: ${e.message}")
                store.retry(now + Timeline.RETRY)
                timeline
            }
        }
        val manager = AppWidgetManager.getInstance(context)
        val placed = ids ?: manager.getAppWidgetIds(ComponentName(context, providerOf(context, kind) ?: return))
        var due = store.due()
        timeline?.next(now)?.let { due = minOf(due, it) }
        var instant = Double.POSITIVE_INFINITY
        for (id in placed) render(context, manager, kind, id, timeline, now)?.let { instant = minOf(instant, it) }
        alarm(context, kind, minOf(due, approach(instant, now)))
    }

    /** When to wake for a render due at [instant]. Android lets an inexact alarm run up to three quarters of its lead
     * late, so one set far ahead of a countdown's end would leave the countdown below zero for minutes (D6). Until the
     * instant is within [LAST_LEAD], the alarm is set halfway there and set again when it runs; the last one is then at
     * most 45 seconds late. */
    internal fun approach(instant: Double, now: Double): Double =
        if (instant - now > LAST_LEAD) now + (instant - now) / 2 else instant

    /** Render placement [id] from [timeline]'s current entry (the preview without one); when the render next changes
     * by itself (a countdown's instant), or null. */
    private fun render(context: Context, manager: AppWidgetManager, kind: String, id: Int, timeline: Timeline?, now: Double): Double? {
        val sizes = sizes(manager, id)
        val row = Row.of(context, kind)
        val props = timeline?.current(now)?.props ?: row?.preview
        val configuration = Store.configuration(context, id) ?: row?.defaults()?.toString()
        val flat = FloatArray(sizes.size * 2) { i -> maxOf(0f, (if (i % 2 == 0) sizes[i / 2].width else sizes[i / 2].height) - 2 * INSET) }
        val answer = JSONObject(Native.render(dir(context), kind.toByteArray(), props?.toByteArray(), configuration?.toByteArray(), flat,
            now, context.resources.configuration.fontScale).decodeToString())
        if (answer.has("error")) {
            Log.w(TAG, "widget $kind: render: ${answer.getString("error")}")
            return null
        }
        val rendered = answer.getJSONArray("sizes")
        val elapsed = SystemClock.elapsedRealtime()
        val links = links(context, id)
        var next: Double? = null
        val views = try {
            val variants = LinkedHashMap<SizeF, RemoteViews>()
            for (i in 0 until rendered.length()) {
                val root = Node.parse(rendered.getJSONObject(i).getJSONObject("root"))
                next = listOfNotNull(next, changes(root, now)).minOrNull()
                variants[sizes[i]] = Inflater(context, links, now, elapsed).widget(root, INSET)
            }
            if (variants.size == 1) variants.values.first() else RemoteViews(variants)
        } catch (e: Unsupported) {
            Log.w(TAG, "widget $kind: ${e.message}")
            return null
        }
        val bytes = Parcel.obtain().let { p -> views.writeToParcel(p, 0); p.dataSize().also { p.recycle() } }
        if (bytes > MAX_BYTES) {
            Log.w(TAG, "widget $kind: $bytes bytes of views, past $MAX_BYTES")
            return null
        }
        manager.updateAppWidget(id, views)
        return next
    }

    /** The earliest instant a countdown in [node] reaches, after which it shows the kernel's text (D6). */
    private fun changes(node: Node, now: Double): Double? =
        listOfNotNull(node.time?.datetime?.takeIf { node.time.format == "countdown" && it > now },
            node.time?.takeIf { it.format == "relative" }?.let { now + 60_000.0 },
            node.children.mapNotNull { changes(it, now) }.minOrNull()).minOrNull()

    /** The placement's sizes in dp (`OPTION_APPWIDGET_SIZES`, else the portrait and landscape sizes), at most 16. */
    private fun sizes(manager: AppWidgetManager, id: Int): List<SizeF> {
        val options = manager.getAppWidgetOptions(id)
        @Suppress("DEPRECATION")
        val listed = options.getParcelableArrayList<SizeF>(AppWidgetManager.OPTION_APPWIDGET_SIZES).orEmpty()
        if (listed.isNotEmpty()) return listed.distinct().take(16)
        val minW = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
        val maxW = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH)
        val minH = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT)
        val maxH = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT)
        if (minW == 0 || minH == 0) return listOf(SizeF(110f, 110f))
        return listOf(SizeF(minW.toFloat(), maxH.toFloat()), SizeF(maxW.toFloat(), minH.toFloat())).distinct()
    }

    /** A placement's taps (D6): a link opens the app's activity at its URL, anywhere else the app. */
    private fun links(context: Context, id: Int): Links = object : Links {
        private fun activity(data: Uri?, code: Int): PendingIntent {
            val intent = Intent(Intent.ACTION_VIEW).setClassName(context, "dev.exact.app.MainActivity")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            if (data != null) intent.data = data else intent.action = Intent.ACTION_MAIN
            return PendingIntent.getActivity(context, code, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }

        override fun open() = activity(null, id * 64)
        override fun link(href: String, index: Int) = activity(Uri.parse(href), id * 64 + 1 + index)
    }

    /** One alarm per kind, at the earliest render or refresh due; inexact and not waking (D5). */
    private fun alarm(context: Context, kind: String, at: Double) {
        val provider = providerOf(context, kind) ?: return
        val intent = Intent(Provider.ACTION_TICK).setClassName(context, provider).setData(Uri.parse("exact-widget:$kind"))
        val pending = PendingIntent.getBroadcast(context, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val alarms = context.getSystemService(AlarmManager::class.java)
        alarms.set(AlarmManager.RTC, at.toLong(), pending)
    }

    /** The provider class of [kind], as the app's manifest names it (`dev.exact.widget.kind` meta-data). */
    fun providerOf(context: Context, kind: String): String? =
        AppWidgetManager.getInstance(context).getInstalledProvidersForPackage(context.packageName, null)
            .firstOrNull { Provider.kindOf(context, it.provider.className) == kind }?.provider?.className

    private fun dir(context: Context): ByteArray {
        android.system.Os.setenv("XDG_CACHE_HOME", context.cacheDir.absolutePath, true)
        android.system.Os.setenv("EXACT_PACKAGE", context.packageName, true)
        return context.filesDir.absolutePath.toByteArray()
    }

    /** Half the binder's 1 MB transaction (D2). */
    private const val MAX_BYTES = 500_000
}

/** A kind's kept timeline and when its next refresh is due, and each placement's chosen configuration, outside backup
 * (`noBackupFilesDir/exact-widgets`): a timeline can hold account content. The timeline is kept with the APK it was
 * made under, so an update of the APK discards it (D8). */
internal class Store(private val context: Context, private val kind: String) {
    private val file = File(root(context), "$kind.json")

    private fun read(): JSONObject? = runCatching { JSONObject(file.readText()) }.getOrNull()?.takeIf { it.optString("cohort") == cohort(context) }

    fun timeline(): Timeline? = read()?.optJSONArray("timeline")?.let { runCatching { Timeline.admit(it.toString()) }.getOrNull() }

    fun due(): Double = read()?.optDouble("due", 0.0) ?: 0.0

    fun keep(timeline: Timeline, due: Double) = write(JSONObject().put("cohort", cohort(context)).put("timeline", org.json.JSONArray(timeline.json())).put("due", due))

    fun retry(due: Double) = write((read() ?: JSONObject().put("cohort", cohort(context))).put("due", due))

    private fun write(o: JSONObject) {
        val temporary = File(file.parentFile, ".${file.name}.tmp")
        temporary.writeText(o.toString())
        temporary.renameTo(file)
    }

    companion object {
        private fun root(context: Context) = File(context.noBackupFilesDir, "exact-widgets").apply { mkdirs() }

        private fun cohort(context: Context): String {
            val info = context.packageManager.getPackageInfo(context.packageName, 0)
            return "${info.longVersionCode}-${info.lastUpdateTime}"
        }

        private fun configurationFile(context: Context, id: Int) = File(root(context), "configuration-$id.json")

        fun configuration(context: Context, id: Int): String? = runCatching { configurationFile(context, id).readText() }.getOrNull()

        fun configure(context: Context, id: Int, choices: JSONObject) = configurationFile(context, id).writeText(choices.toString())

        fun forget(context: Context, id: Int) { configurationFile(context, id).delete() }

        fun move(context: Context, from: Int, to: Int) { configurationFile(context, from).renameTo(configurationFile(context, to)) }
    }
}
