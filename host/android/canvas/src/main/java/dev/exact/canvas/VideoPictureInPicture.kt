package dev.exact.canvas

import android.app.PictureInPictureParams
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Rect
import android.util.Rational
import android.view.View
import android.view.ViewGroup

/** The pinned activity shows one existing player's video above its page (LLP 1042 §9.3). */
internal class VideoPictureInPicture(private val activity: ExactActivity) {
    private class Presentation(val target: MediaAllocation, val player: VideoPlayer) {
        var entered = false
        var removed = false
        var view: View? = null
    }
    private var current: Presentation? = null
    private var expanding = false
    private val commands = ArrayDeque<MediaAllocation?>()

    // Android can report isInPictureInPictureMode before it delivers the entry callback.
    // Only the callback releases the next command. A null target means exit.
    fun request(target: MediaAllocation?) {
        commands.addLast(target)
        drain()
    }

    private fun drain() {
        while (commands.isNotEmpty()) {
            val presentation = current
            if (expanding || presentation?.entered == false) return
            val next = commands.first()
            if (presentation != null && next != presentation.target) {
                if (next == null) commands.removeFirst()
                expand()
                return
            }
            commands.removeFirst()
            if (next != null && presentation == null) activity.floatPicture(next)
        }
    }

    fun start(target: MediaAllocation, player: VideoPlayer, source: Rect) {
        fun refuse(reason: String) = target.tell("pictureinpictureerror", reason, "")
        if (player.pictureDisabled) { refuse("this video disables picture in picture"); return }
        if (!activity.packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) {
            refuse("picture in picture is unavailable on this Android device")
            return
        }
        val size = player.pictureSize
        if (size == null || size.width <= 0 || size.height <= 0) {
            refuse("the video has no loaded picture")
            return
        }
        current = Presentation(target, player)
        try {
            if (!activity.enterPictureInPictureMode(params(size, source))) {
                current = null
                refuse("Android declined picture in picture")
            }
        } catch (error: RuntimeException) {
            current = null
            refuse(error.message ?: "Android declined picture in picture")
        }
    }

    private fun params(size: android.util.Size, source: Rect? = null): PictureInPictureParams {
        val ratio = Rational(size.width.coerceAtLeast(1), size.height.coerceAtLeast(1)).coerceIn(Rational(100, 239), Rational(239, 100))
        return PictureInPictureParams.Builder().setAspectRatio(ratio).apply {
            if (source != null && !source.isEmpty) setSourceRectHint(source)
        }.build()
    }

    fun modeChanged(active: Boolean) {
        if (!active) {
            ended()
        } else {
            val presentation = current ?: return
            if (presentation.entered) return
            presentation.entered = true
            presentation.target.tell("enterpictureinpicture", "", "")
            if (!presentation.removed) {
                try {
                    val view = presentation.player.floatPicture { size ->
                        if (current === presentation && !expanding) activity.setPictureInPictureParams(params(size))
                    } ?: throw IllegalStateException("the video player is unavailable")
                    presentation.view = view
                    activity.addContentView(view, ViewGroup.LayoutParams(-1, -1))
                } catch (error: RuntimeException) {
                    presentation.target.tell("pictureinpictureerror", error.message ?: "Android could not show the video", "")
                    commands.addFirst(null)
                }
            }
        }
        // Window changes requested by the callback run after Android applies this configuration.
        activity.window.decorView.post { drain() }
    }

    /** Bringing the same singleTop activity forward expands the pinned task. */
    private fun expand() {
        expanding = true
        activity.startActivity(Intent(activity, activity.javaClass).addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP))
    }

    fun removed(target: MediaAllocation?) {
        commands.removeAll { it != null && it == target }
        val presentation = current ?: return
        if (target != presentation.target || presentation.removed) return
        presentation.removed = true
        if (presentation.view != null) presentation.player.restorePicture()
        if (!activity.isFinishing && !activity.isDestroyed && !expanding) {
            commands.addFirst(null)
            drain()
        }
    }

    fun destroy() {
        commands.clear()
        ended()
    }

    private fun ended() {
        expanding = false
        val presentation = current ?: return
        current = null
        val view = presentation.view
        if (view != null) {
            if (!presentation.removed) presentation.player.restorePicture()
            // The canvas can retire a node while the activity's content is drawing its children.
            val parent = view.parent as? ViewGroup
            parent?.post { if (view.parent === parent) parent.removeView(view) }
        }
        if (presentation.entered) presentation.target.tell("leavepictureinpicture", "", "")
    }
}
