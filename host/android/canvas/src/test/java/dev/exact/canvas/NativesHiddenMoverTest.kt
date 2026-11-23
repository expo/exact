package dev.exact.canvas

import android.app.Activity
import android.view.View
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File

/** A video in a hidden `portal` mover keeps its player, out of view, and shows the same one again (LLP 1013.002 D5). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class NativesHiddenMoverTest {
    private class Player(context: android.content.Context) : VideoPlayer {
        var released = false
        val shown = ArrayList<Boolean>()
        var hidden = 0
        override val view: View = View(context)
        override val lendable = false
        override val engaged = false
        override val controller: View? = null
        override fun bind(sink: MediaSink) {}
        override fun update(props: JSONObject, width: Int, height: Int, from: MediaRecord.Resume?) {}
        override fun shown(inView: Boolean) { shown += inView }
        override fun hidden() { hidden++ }
        override fun snapshot(): MediaRecord.Resume? = null
        override fun stop() {}
        override fun release() { released = true }
    }

    private val props = JSONObject().put("src", "assets/motion.mp4").put("autoplay", true).put("muted", true)
        .put("generation", 1).put("slot", 1).put("session", 1).toString()

    @Test fun aHiddenMoversVideoKeepsItsPlayerOutOfView() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val players = ArrayList<Player>()
        val natives = Natives(activity, { File(".") }, emptyMap(), { context -> Player(context).also { players.add(it) } })
        try {
            natives.ensure(1, 0, 100, 50, props, 0)
            natives.unrowed(setOf(1))
            val player = players.single()
            assertEquals(listOf(true), player.shown)
            // Hidden: an empty box, kept at its size, told once (it leaves fullscreen).
            for (frame in 0 until 2) {
                natives.ensure(1, 0, 0, 0, props, 0, hidden = true)
                natives.unrowed(setOf(1))
            }
            assertEquals(listOf(true, false), player.shown)
            assertEquals(1, player.hidden)
            assertFalse(player.released)
            assertEquals(100, player.view.width)
            // Shown again: the same player, in view.
            natives.ensure(1, 0, 100, 50, props, 0)
            natives.unrowed(setOf(1))
            assertEquals(listOf(true, false, true), player.shown)
            assertEquals(1, players.size)
            // Its declaration gone: the stream no longer names it, and it goes.
            natives.unrowed(emptySet())
            assertEquals(true, player.released)
        } finally {
            natives.destroyAll()
        }
    }
}
