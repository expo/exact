package dev.exact.canvas

import android.os.Bundle
import android.view.View

/** Activity callbacks for module views, including views made after resume (LLP 1133.021 §9 item 4). */
enum class ViewLifecycle { CREATE, START, RESUME, PAUSE, STOP, DESTROY, SAVE_INSTANCE_STATE, LOW_MEMORY }

internal class HostedLifecycle {
    private data class Entry(var key: String?, val module: NativeModule)
    private val views = LinkedHashMap<View, Entry>()
    private var saved = Bundle()
    private var started = false
    private var resumed = false

    fun restore(state: Bundle?) { saved = state?.let(::Bundle) ?: Bundle() }

    fun awaiting(key: String): Boolean = saved.containsKey(key)

    fun made(view: View, key: String, module: NativeModule) {
        val existing = views[view]
        if (existing != null) { existing.key = key; return }
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
        for ((view, entry) in views.toMap()) entry.module.lifecycle(view, event)
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
        if (resumed) entry.module.lifecycle(view, ViewLifecycle.PAUSE)
        if (started) entry.module.lifecycle(view, ViewLifecycle.STOP)
        entry.module.lifecycle(view, ViewLifecycle.DESTROY)
    }
}
