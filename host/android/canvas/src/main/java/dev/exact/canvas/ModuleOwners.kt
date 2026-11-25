package dev.exact.canvas

import android.os.Handler
import android.os.Looper
import java.util.concurrent.Callable
import java.util.concurrent.FutureTask
import android.os.Bundle
import android.view.View
import androidx.activity.BackEventCompat
import androidx.activity.OnBackPressedCallback
import androidx.activity.OnBackPressedDispatcher
import androidx.activity.OnBackPressedDispatcherOwner
import androidx.activity.setViewTreeOnBackPressedDispatcherOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner

/** Made at the first module or hatch view, never at launch (LLP 1115.001.000 D2). */
internal class ModuleOwners(
    root: View, state: Bundle?, retained: Any?, leave: () -> Unit,
    private val commit: (Int, Int) -> Int, private val progress: (Int, Int, Int, Float, Int) -> Unit,
) : SavedStateRegistryOwner, ViewModelStoreOwner, OnBackPressedDispatcherOwner {
    override val onBackPressedDispatcher = OnBackPressedDispatcher(leave)
    private inner class Back(val kind: Int, val id: Int, enabled: Boolean) : OnBackPressedCallback(enabled) {
        var retired = false
        override fun handleOnBackPressed() {
            if (commit(kind, id) < 0) retryPress()
        }
        override fun handleOnBackStarted(backEvent: BackEventCompat) {
            if (kind <= SHEET) progress(kind, id, 0, backEvent.progress, backEvent.swipeEdge)
        }
        override fun handleOnBackProgressed(backEvent: BackEventCompat) {
            if (kind <= SHEET) progress(kind, id, 1, backEvent.progress, backEvent.swipeEdge)
        }
        override fun handleOnBackCancelled() {
            // Removing the previewed sheet already cancelled its preview in the presenter.
            if (!retired && kind <= SHEET) progress(kind, id, 2, 0f, 0)
        }
    }
    private val route = Back(1, 1, false)
    private val layers = mutableMapOf<Pair<Int, Int>, Back>()
    private val handoff = BackHandoff(::update)
    private var subscribed = false
    private val registry = LifecycleRegistry(this)
    private val saved = SavedStateRegistryController.create(this)
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry get() = saved.savedStateRegistry
    override val viewModelStore = retained as? ViewModelStore ?: ViewModelStore()

    init {
        saved.performAttach()
        saved.performRestore(state)
        // Compose's window recomposer reads the content child, rather than the requesting view.
        root.setViewTreeLifecycleOwner(this)
        root.setViewTreeSavedStateRegistryOwner(this)
        root.setViewTreeViewModelStoreOwner(this)
        root.setViewTreeOnBackPressedDispatcherOwner(this)
        onBackPressedDispatcher.addCallback(route)
    }

    fun subscribe() {
        if (subscribed) return
        Exact.backHandoff = handoff
        subscribed = Exact.hostedBack(true)
    }

    fun unsubscribe() {
        if (Exact.backHandoff !== handoff) return
        Exact.hostedBack(false)
        Exact.backHandoff = null
        subscribed = false
    }

    private fun update(targets: IntArray) {
        val routeEnabled = targets[0] != 0
        val active = targets[1]
        val wanted = mutableMapOf<Pair<Int, Int>, Boolean>()
        for (index in 3 until targets.size step 2) {
            val kind = targets[index]
            val id = targets[index + 1]
            wanted[kind to id] = active == kind && targets[2] == id
        }
        // Enable the successor first so AndroidX keeps its platform registration across a handoff.
        if (routeEnabled) route.isEnabled = true
        for ((key, enabled) in wanted) {
            val callback = layers.getOrPut(key) {
                Back(key.first, key.second, enabled).also(onBackPressedDispatcher::addCallback)
            }
            if (enabled) callback.isEnabled = true
        }
        layers.entries.removeAll { (key, callback) ->
            when (wanted[key]) {
                null -> { callback.retired = true; callback.remove(); true }
                false -> { callback.isEnabled = false; false }
                true -> false
            }
        }
        if (!routeEnabled) route.isEnabled = false
    }

    private var retrying = false
    private var retry = false

    /** An earlier presenter turn can retire AndroidX's choice while the press is on its way
     * to that thread. Its successor is already registered when the commit returns. */
    private fun retryPress() {
        retry = true
        if (retrying) return
        retrying = true
        try {
            do {
                retry = false
                onBackPressedDispatcher.onBackPressed()
            } while (retry)
        } finally { retrying = false }
    }

    fun hasWindow(serial: Int): Boolean = layers.containsKey(5 to serial)

    fun receiveBack() {
        handoff.flush()
        onBackPressedDispatcher.onBackPressed()
    }

    fun move(phase: Int) {
        registry.currentState = when (phase) {
            3 -> Lifecycle.State.RESUMED
            2 -> Lifecycle.State.STARTED
            else -> Lifecycle.State.CREATED
        }
    }

    fun save(): Bundle {
        registry.currentState = Lifecycle.State.CREATED
        return Bundle().also { saved.performSave(it) }
    }

    fun destroy(recreating: Boolean) {
        registry.currentState = Lifecycle.State.DESTROYED
        if (!recreating) viewModelStore.clear()
    }

    companion object {
        private const val SHEET = 2
        fun clear(retained: Any) { (retained as? ViewModelStore)?.clear() }
    }
}

/** The presenter requests a layer before it publishes its content. AndroidX registrations
 * belong to the main thread, which may itself be waiting for that presenter turn. */
internal class BackHandoff(private val update: (IntArray) -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val monitor = Object()
    private var pending: FutureTask<Unit>? = null

    fun request(layers: IntArray) {
        if (Looper.myLooper() == main.looper) { update(layers); return }
        val request = FutureTask { update(layers) }
        synchronized(monitor) { pending = request; monitor.notifyAll() }
        main.post { flush() }
        try { request.get() } finally {
            synchronized(monitor) { if (pending === request) pending = null }
        }
    }

    fun flush() {
        val request = synchronized(monitor) { pending.also { pending = null } }
        request?.run()
    }

    /** Service just the registration, which never calls back into the borrowed presenter. */
    fun <T> awaitOwner(owner: Handler, waitMs: Long? = null, work: () -> T): T {
        val result = object : FutureTask<T>(Callable { work() }) {
            override fun done() { synchronized(monitor) { monitor.notifyAll() } }
        }
        val deadline = waitMs?.let { System.nanoTime() + it * 1_000_000 }
        owner.post(result)
        while (!result.isDone) {
            val request = synchronized(monitor) {
                while (pending == null && !result.isDone) {
                    if (deadline == null) monitor.wait() else {
                        val remaining = deadline - System.nanoTime()
                        if (remaining <= 0) throw java.util.concurrent.TimeoutException()
                        java.util.concurrent.TimeUnit.NANOSECONDS.timedWait(monitor, remaining)
                    }
                }
                if (!result.isDone && deadline != null && System.nanoTime() >= deadline) {
                    throw java.util.concurrent.TimeoutException()
                }
                pending.also { pending = null }
            }
            request?.run()
        }
        return result.get()
    }
}
