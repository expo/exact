package dev.exact.canvas

import android.graphics.RenderNode
import java.nio.IntBuffer
import java.util.WeakHashMap

/** HWUI may discard the display list of a row that leaves the frame during a reorder lift.
 * The row's next draw can name the same node without sending its commands again. */
internal class RowDrawings(private val player: () -> Player) {
    private val commands = WeakHashMap<RenderNode, IntArray>()

    /** Copy the body while its stream is live. The worker can replay the same copy. */
    fun save(node: RenderNode, stream: IntBuffer, from: Int, to: Int): IntArray =
        IntArray(to - from) { stream.get(from + it) }.also { commands[node] = it }

    fun record(node: RenderNode, stream: IntBuffer, from: Int, to: Int, player: Player) {
        save(node, stream, from, to)
        val canvas = node.beginRecording()
        try { player.replay(canvas, stream, from, to, into = node) } finally { node.endRecording() }
    }

    fun drawn(node: RenderNode): RenderNode {
        if (!node.hasDisplayList()) commands[node]?.let { words ->
            // A row can be requested from inside another player's replay.
            val canvas = node.beginRecording()
            try { player().replay(canvas, IntBuffer.wrap(words), 0, words.size, into = node) }
            finally { node.endRecording() }
        }
        return node
    }
}
