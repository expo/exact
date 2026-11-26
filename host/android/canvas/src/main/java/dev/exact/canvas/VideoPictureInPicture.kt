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
    private data class Presentation(val target: MediaAllocation, val player: VideoPlayer, var view: View? = null, var removed: Boolean = false)
    private var current: Presentation? = null
    private var expandOnEntry = false

    fun start(target: MediaAllocation, player: VideoPlayer, source: Rect) {
        fun refuse(reason: String) = target.tell("pictureinpictureerror", reason, "")
        if (current?.target == target) return
        if (current != null && current?.view == null) {
            refuse("another video is entering picture in picture")
            return
        }
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
        val params = params(size, source)
        try {
            if (activity.isInPictureInPictureMode) {
                activity.setPictureInPictureParams(params)
                ended()
                current = Presentation(target, player)
                modeChanged(true)
            } else {
                current = Presentation(target, player)
                expandOnEntry = false
                if (!activity.enterPictureInPictureMode(params)) {
                    ended()
                    refuse("Android declined picture in picture")
                }
            }
        } catch (error: RuntimeException) {
            if (current?.target == target) ended()
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
        if (!active) { expandOnEntry = false; ended(); return }
        val exitRequested = expandOnEntry
        expandOnEntry = false
        val presentation = current
        if (presentation == null) { if (exitRequested) expand(); return }
        if (presentation.view != null) return
        val view = try {
            val surface = presentation.player.floatPicture { size ->
                if (current === presentation && activity.isInPictureInPictureMode) activity.setPictureInPictureParams(params(size))
            } ?: throw IllegalStateException("the video player is unavailable")
            activity.addContentView(surface, ViewGroup.LayoutParams(-1, -1))
            surface
        } catch (error: RuntimeException) {
            presentation.player.restorePicture()
            current = null
            presentation.target.tell("pictureinpictureerror", error.message ?: "Android could not show the video", "")
            expand()
            return
        }
        presentation.view = view
        presentation.target.tell("enterpictureinpicture", "", "")
        if (exitRequested) view.post { if (current === presentation) expand() }
    }

    /** Bringing the same singleTop activity forward expands the pinned task. */
    fun expand() {
        if (activity.isInPictureInPictureMode) {
            activity.startActivity(Intent(activity, activity.javaClass).addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP))
        } else if (current != null) expandOnEntry = true
        else android.util.Log.i("exact", "exitPictureInPicture ignored: nothing is floating")
    }

    fun removed(target: MediaAllocation?) {
        val presentation = current ?: return
        if (target != presentation.target || presentation.removed) return
        presentation.removed = true
        if (presentation.view != null) presentation.player.restorePicture()
        if (!activity.isFinishing && !activity.isDestroyed) expand()
        if (presentation.view == null) current = null
    }

    fun ended() {
        val presentation = current ?: return
        current = null
        val view = presentation.view ?: return
        if (!presentation.removed) presentation.player.restorePicture()
        // The canvas can retire a node while the activity's content is drawing its children.
        val parent = view.parent as? ViewGroup
        parent?.post { if (view.parent === parent) parent.removeView(view) }
        presentation.target.tell("leavepictureinpicture", "", "")
    }
}
