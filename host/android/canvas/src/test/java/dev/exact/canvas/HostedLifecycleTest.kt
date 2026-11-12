package dev.exact.canvas

import android.content.Context
import android.os.Bundle
import android.view.View
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class HostedLifecycleTest {
    private class Module(val reuse: Boolean = false) : NativeModule {
        val events = ArrayList<Pair<View, ViewLifecycle>>()
        val restored = ArrayList<Int>()
        val cameras = HashMap<View, Int>()
        var made = 0
        var destroyed = 0
        override fun create(context: Context, props: JSONObject): View = View(context).also { made++ }
        override fun update(view: View, props: JSONObject) {}
        override fun reset(view: View) = reuse
        override fun destroy(view: View) { destroyed++ }
        override fun lifecycle(view: View, event: ViewLifecycle, state: Bundle?) {
            events.add(view to event)
            if (event == ViewLifecycle.SAVE_INSTANCE_STATE) state!!.putInt("camera", cameras[view] ?: 42)
            if (event == ViewLifecycle.CREATE) {
                restored.add(state?.getInt("camera") ?: 0)
                if (state != null) cameras[view] = state.getInt("camera")
            }
        }
        fun sequence() = events.map { it.second }
    }
    private fun natives(module: Module) = Natives(RuntimeEnvironment.getApplication(), File("."), mapOf("map" to module))
    private fun Natives.map(id: Int = 7, row: Int = 0) = ensure(id, 2, 100, 100, "{\"module\":\"map\",\"slot\":2,\"generation\":3}", row)

    @Test fun lateViewsCatchUpAndGetEachActivityCallbackAndSavedState() {
        val module = Module()
        val host = natives(module)
        host.lifecycle.event(ViewLifecycle.START)
        host.lifecycle.event(ViewLifecycle.RESUME)
        host.map()
        host.lifecycle.event(ViewLifecycle.LOW_MEMORY)
        val state = host.lifecycle.save()
        host.lifecycle.event(ViewLifecycle.PAUSE)
        host.lifecycle.event(ViewLifecycle.STOP)
        host.destroyAll()
        assertEquals(listOf(ViewLifecycle.CREATE, ViewLifecycle.START, ViewLifecycle.RESUME, ViewLifecycle.LOW_MEMORY,
            ViewLifecycle.SAVE_INSTANCE_STATE, ViewLifecycle.PAUSE, ViewLifecycle.STOP, ViewLifecycle.DESTROY), module.sequence())
        assertEquals(1, module.destroyed)
        val restored = natives(module)
        restored.lifecycle.restore(state)
        restored.map()
        assertEquals(listOf(0, 42), module.restored)
        restored.destroyAll()
    }

    @Test fun pooledViewsCannotOverwriteTheSavedStateOfAReboundAllocation() {
        val module = Module(reuse = true)
        val host = natives(module)
        host.map(7, row = 70)
        host.map(8, row = 80)
        val a = host.taker(7, 2, 3)!!
        val b = host.taker(8, 2, 3)!!
        module.cameras[a] = 11
        module.cameras[b] = 22
        host.lifecycle.event(ViewLifecycle.START)
        host.lifecycle.event(ViewLifecycle.RESUME)
        host.near(setOf(70), setOf(70), setOf(70, 80))
        host.near(emptySet(), emptySet(), setOf(70, 80))
        host.near(setOf(80), setOf(80), setOf(70, 80))
        assertSame(a, host.taker(8, 2, 3))
        assertNull(host.taker(7, 2, 3))
        assertEquals(2, module.made)
        module.cameras[a] = 33
        val saved = host.lifecycle.save()
        assertEquals(33, saved.getBundle("map:8:2:3")!!.getInt("camera"))
        assertEquals(setOf("map:8:2:3"), saved.keySet())
        assertFalse(module.events.contains(b to ViewLifecycle.SAVE_INSTANCE_STATE))
        host.lifecycle.event(ViewLifecycle.LOW_MEMORY)
        host.lifecycle.event(ViewLifecycle.PAUSE)
        host.lifecycle.event(ViewLifecycle.STOP)
        host.destroyAll()
        for (view in listOf(a, b)) assertEquals(listOf(ViewLifecycle.CREATE, ViewLifecycle.START,
            ViewLifecycle.RESUME) + (if (view === a) listOf(ViewLifecycle.SAVE_INSTANCE_STATE) else emptyList()) +
            listOf(ViewLifecycle.LOW_MEMORY, ViewLifecycle.PAUSE, ViewLifecycle.STOP, ViewLifecycle.DESTROY),
            module.events.filter { it.first === view }.map { it.second })
        val restored = natives(module)
        restored.lifecycle.restore(saved)
        restored.map(8)
        assertEquals(listOf(0, 0, 33), module.restored)
        restored.destroyAll()
    }

    @Test fun pooledViewsHaveOneLifecycleAndDestroyedViewsAreNotCalledAgain() {
        val module = Module(reuse = true)
        val host = natives(module)
        host.map()
        host.lifecycle.event(ViewLifecycle.START)
        host.lifecycle.event(ViewLifecycle.RESUME)
        host.unrowed(emptySet())
        host.map(8)
        assertEquals(1, module.made)
        host.destroyAll()
        assertEquals(listOf(ViewLifecycle.CREATE, ViewLifecycle.START, ViewLifecycle.RESUME,
            ViewLifecycle.PAUSE, ViewLifecycle.STOP, ViewLifecycle.DESTROY), module.sequence())
        host.lifecycle.event(ViewLifecycle.LOW_MEMORY)
        assertEquals(ViewLifecycle.DESTROY, module.sequence().last())
        assertEquals(1, module.destroyed)
    }
    @Test fun restoredAllocationsNeverBorrowAViewBeforeReceivingTheirSavedBundle() {
        val module = Module(reuse = true)
        val host = natives(module)
        host.lifecycle.restore(Bundle().also { state ->
            state.putBundle("map:7:2:3", Bundle().also { it.putInt("camera", 11) })
            state.putBundle("map:8:2:3", Bundle().also { it.putInt("camera", 22) })
        })
        host.map(7, row = 70)
        val a = host.taker(7, 2, 3)!!
        host.near(setOf(70), setOf(70), setOf(70))
        host.near(emptySet(), emptySet(), setOf(70))
        host.map(8, row = 80)
        host.near(setOf(80), setOf(80), setOf(70, 80))
        val b = host.taker(8, 2, 3)!!
        assertEquals(listOf(11, 22), module.restored)
        assertNotSame(a, b)
        assertEquals(22, host.lifecycle.save().getBundle("map:8:2:3")!!.getInt("camera"))
        host.destroyAll()
    }
}
