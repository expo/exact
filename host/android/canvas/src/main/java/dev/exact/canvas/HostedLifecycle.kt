package dev.exact.canvas

import android.content.Context
import android.os.Bundle
import android.view.View
import org.json.JSONObject

/** One view factory per module name (LLP 1024). [reset] readies a released view for another element (LLP 1068 §4.8).
 * Returning false destroys the view. */
interface NativeModule {
    fun create(context: Context, props: JSONObject): View
    /** [create] for element [id], the node's id: a package's view is made with it (LLP 1133.008 D8 item 1). */
    fun create(context: Context, props: JSONObject, id: Int): View = create(context, props)
    fun update(view: View, props: JSONObject)
    /** Bound before the first update. Keep this callback only until the next binding; a retired one is ignored. */
    fun events(view: View, emit: (String, String) -> Unit) {}
    /** Bind the preferred content size of this allocation before its first update. */
    fun sizing(view: View, events: NativeViewSizing) {}
    /** Activity callbacks; CREATE receives saved state, SAVE_INSTANCE_STATE fills its private bundle (LLP 1133.021). */
    fun lifecycle(view: View, event: ViewLifecycle, state: android.os.Bundle? = null) {}
    fun reset(view: View): Boolean = false
    fun destroy(view: View) {}
}

/** The app's optional module roster, compiled from `modules/android` as `class exact.app.Modules`. */
interface NativeModules {
    val modules: Map<String, NativeModule>

    companion object {
        /** Construct the app's roster at its first module view. */
        fun load(): Map<String, NativeModule> = AppModules()
        internal fun available(modules: Map<String, NativeModule>) = if (modules is AppModules) modules.type != null else modules.isNotEmpty()
        private class AppModules : AbstractMap<String, NativeModule>() {
            val type by lazy { try { Class.forName("exact.app.Modules", false, NativeModules::class.java.classLoader) } catch (_: ClassNotFoundException) { null } }
            override val entries: Set<Map.Entry<String, NativeModule>> by lazy {
                val type = type ?: return@lazy emptySet()
                try {
                    (type.getDeclaredConstructor().newInstance() as NativeModules).modules.entries
                } catch (e: Exception) {
                    val cause = (e as? java.lang.reflect.InvocationTargetException)?.targetException ?: e
                    android.util.Log.e("exact", "native modules: exact.app.Modules could not be made: $cause")
                    emptySet()
                }
            }
        }
    }
}

/** Activity callbacks for module views, including views made after resume (LLP 1133.021 §9 item 4). */
enum class ViewLifecycle { CREATE, START, RESUME, PAUSE, STOP, DESTROY, SAVE_INSTANCE_STATE, LOW_MEMORY }

/** The module factories and live lifecycle registrations belong to one activity across reader remakes. */
internal class HostedModules(val modules: Map<String, NativeModule>) {
    val lifecycle = HostedLifecycle()
}

internal class HostedLifecycle {
    private data class Entry(var key: String?, val module: NativeModule, var awaitingPaint: Boolean = false)
    private val views = LinkedHashMap<View, Entry>()
    private var saved = Bundle()
    private var started = false
    private var resumed = false

    fun restore(state: Bundle?) { saved = state?.let(::Bundle) ?: Bundle() }

    fun awaiting(key: String): Boolean = saved.containsKey(key)

    fun awaitPaint(view: View) { views[view]?.awaitingPaint = true }

    /** A row parked on the returning reader's first placement still owns its saved bundle when it next mounts. */
    fun saveBeforeFirstPlacement(view: View): String? {
        val entry = views[view]?.takeIf { it.awaitingPaint } ?: return null
        val key = entry.key ?: return null
        saved.putBundle(key, Bundle().also { entry.module.lifecycle(view, ViewLifecycle.SAVE_INSTANCE_STATE, it) })
        return key
    }

    fun forget(key: String) { saved.remove(key) }

    fun painted(view: View) {
        val entry = views[view] ?: return
        if (!entry.awaitingPaint) return
        entry.awaitingPaint = false
        if (started) entry.module.lifecycle(view, ViewLifecycle.START)
        if (resumed) entry.module.lifecycle(view, ViewLifecycle.RESUME)
    }

    fun made(view: View, key: String, module: NativeModule) {
        val existing = views[view]
        if (existing != null) { existing.key = key; painted(view); return }
        views[view] = Entry(key, module)
        module.lifecycle(view, ViewLifecycle.CREATE, saved.getBundle(key))
        saved.remove(key)
        if (started) module.lifecycle(view, ViewLifecycle.START)
        if (resumed) module.lifecycle(view, ViewLifecycle.RESUME)
    }

    fun unbind(view: View) { views[view]?.key = null }

    fun event(event: ViewLifecycle) {
        when (event) {
            ViewLifecycle.START -> { if (started) return; started = true }
            ViewLifecycle.RESUME -> { if (resumed) return; resumed = true }
            ViewLifecycle.PAUSE -> { if (!resumed) return; resumed = false }
            ViewLifecycle.STOP -> { if (!started) return; event(ViewLifecycle.PAUSE); started = false }
            else -> {}
        }
        for ((view, entry) in views.toMap()) {
            if (!entry.awaitingPaint || event == ViewLifecycle.LOW_MEMORY) entry.module.lifecycle(view, event)
        }
    }

    fun save(): Bundle = Bundle(saved).also { out ->
        for ((view, entry) in views) {
            val key = entry.key ?: continue
            val state = Bundle()
            entry.module.lifecycle(view, ViewLifecycle.SAVE_INSTANCE_STATE, state)
            out.putBundle(key, state)
        }
    }

    fun destroyed(view: View) {
        val entry = views.remove(view) ?: return
        if (resumed && !entry.awaitingPaint) entry.module.lifecycle(view, ViewLifecycle.PAUSE)
        if (started && !entry.awaitingPaint) entry.module.lifecycle(view, ViewLifecycle.STOP)
        entry.module.lifecycle(view, ViewLifecycle.DESTROY)
    }
}
