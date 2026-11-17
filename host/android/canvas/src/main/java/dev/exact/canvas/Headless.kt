package dev.exact.canvas

import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import java.io.File

/**
 * The process's host with no activity (LLP 1133.017 D7, D10): what a platform callback boots to raise a wake when
 * nothing shows the app. It is the host an activity leaves at its stop: hidden, it runs tasks and answers calls and
 * records no frame. An activity made later adopts it, as one made again does ([ExactActivity.onCreate]).
 */
internal object Headless {
    private val main = Handler(Looper.getMainLooper())

    /**
     * [ready] hears, on the main thread, whether the process has a booted host whose data module is active, with its
     * `"start": "load"` extensions made: a handler raised from then on has its data calls answered and its storage
     * configured. A process no activity has set up is set up here from what the application alone says: none of the
     * launch extras a development build reads, and no launch URL or share, which the first activity made over the
     * host gives it from its own intent ([ExactActivity.onCreate], [Exact.launch]).
     */
    fun session(context: Context, ready: (Boolean) -> Unit) {
        main.post {
            if (!Exact.live) start(context.applicationContext)
            val owner = Exact.owner
            // owner=0: the host is an activity's, on the main thread.
            if (owner == null) ready(false)
            // Behind the boot, this one's or an activity's, and the data module's activation.
            else owner.active { ok -> if (ok) Extensions.firstFrame(); ready(ok) }
        }
    }

    private fun start(app: Context) {
        Exact.live = true
        Exact.unshown = true
        Setup.environment(app)
        Setup.nativeEnvironment()
        try { FilePicker.start(app) }
        catch (e: Exception) { android.util.Log.w("exact", "file picker: startup cleanup failed", e) }
        val owner = Owner(hwbuf = true, fast = true, android.os.Process.THREAD_PRIORITY_DEFAULT) { null }.also { Exact.owner = it }
        // Stopped from the start: no reader takes a frame.
        owner.running(false)
        Native.trust(app)
        Extensions.bind(app)
        Setup.assets(app)
        owner.configure(Device.read(app))
        owner.colors(Palette.read(app))
        // The display's size stands for a window's: the activity that adopts the host tells its own.
        val metrics = app.resources.displayMetrics
        val scale = Device.scale(app)
        owner.window(metrics.widthPixels, metrics.heightPixels, scale, IntArray(16))
        owner.boot(metrics.widthPixels, metrics.heightPixels, scale, app.filesDir.absolutePath, app.noBackupFilesDir.absolutePath, false,
            started = Exact.hiddenAtBoot()) {}
    }
}

/** What a process's host needs before exact2's library loads and it boots, whoever sets the process up: an activity
 * or [Headless]. */
internal object Setup {
    /** Write exact2's environment as the application alone decides it. Returns the application's meta-data. */
    fun environment(app: Context): Bundle? {
        android.system.Os.setenv("XDG_CACHE_HOME", app.cacheDir.absolutePath, true)
        // Rust's `temp_dir` is TMPDIR, else /data/local/tmp, which an app cannot write. API 29 sets none, and a
        // drive with no scratch store keeps its `app:/` files under it (`exact_painted::picker`).
        android.system.Os.setenv("TMPDIR", app.cacheDir.absolutePath, false)
        val meta = app.packageManager.getApplicationInfo(app.packageName, android.content.pm.PackageManager.GET_META_DATA).metaData
        // The `protocol_handlers` this APK registers (LLP 1038 D8): exact2 puts a URL of one's scheme at its template's
        // `%s`, at launch and while the app runs, as the Apple hosts do with `ExactProtocolHandlers`.
        meta?.getString("dev.exact.protocol_handlers")?.let { android.system.Os.setenv("EXACT_PROTOCOL_HANDLERS", it, true) }
        // GPU modules (LLP 1009) come from this APK. Exported intents never select executable content.
        android.system.Os.setenv("EXACT_NATIVE_LIBS", app.applicationInfo.nativeLibraryDir, true)
        android.system.Os.setenv("EXACT_MOVE_TRACKS", "1", true)
        // The logcat tag of the app's `console` lines, in every build (`host/painted/src/android.rs`, `console`): the
        // package name, as Apple's subsystem is the bundle identifier. A label can be localized and hold spaces.
        android.system.Os.setenv("EXACT_PACKAGE", app.packageName, true)
        return meta
    }

    /** Called once, after the launch extras and before the library's first host thread. */
    fun nativeEnvironment() = Native.environment(android.system.Os.environ().joinToString("\u0000").toByteArray(Charsets.UTF_8))

    /** The APK's `app/` assets (the plan names `assets/…` under it, a GPU module's shaders `shaders/`), copied once
     * per install into the app's files, replacing the last install's: exact2 reads files. */
    fun assets(app: Context) {
        val to = File(app.filesDir, "app")
        val stamp = app.packageManager.getPackageInfo(app.packageName, 0).lastUpdateTime.toString()
        val done = File(to, ".done")
        if (done.exists() && done.readText() == stamp) return
        fun copy(from: String, dir: File) {
            val names = app.assets.list(from) ?: return
            if (names.isEmpty()) {
                dir.parentFile?.mkdirs()
                app.assets.open(from).use { i -> dir.outputStream().use { o -> i.copyTo(o) } }
                return
            }
            dir.mkdirs()
            for (n in names) copy("$from/$n", File(dir, n))
        }
        // A new install's tree replaces the old, an empty one included: a file the APK no longer has (a shader a
        // module no longer names, which the host would register and the module refuse) does not linger.
        to.deleteRecursively()
        if (app.assets.list("app").isNullOrEmpty()) return
        copy("app", to)
        done.writeText(stamp)
    }
}
