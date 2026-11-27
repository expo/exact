package dev.exact.video

import android.app.Dialog
import android.content.Context
import android.graphics.Color
import android.os.Build
import android.view.ContextThemeWrapper
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerView

/**
 * A video full screen (LLP 1042.001 D9): a window of the activity over everything, its system bars hidden as immersive
 * mode hides them, holding Media3's `PlayerView` with its controller on the element's own player. Back and the
 * controller's full-screen button leave; [left] then takes the player back. Full screen is the platform player's
 * presentation; inline object-position belongs to the element's box.
 */
@OptIn(UnstableApi::class)
internal class FullScreen(private val context: Context, private val exo: ExoPlayer, private val left: (PlayerView) -> Unit) {
    private var dialog: Dialog? = null
    private var view: PlayerView? = null

    val on: Boolean get() = dialog != null

    /** Shown, the player taken from [from] (the inline controller), or from the texture the caller has let go. */
    fun enter(from: PlayerView?, fit: String) {
        if (dialog != null) return
        val themed = ContextThemeWrapper(context, android.R.style.Theme_DeviceDefault)
        val made = LayoutInflater.from(themed).inflate(R.layout.exact_video_controls, null, false) as PlayerView
        made.setShowNextButton(false); made.setShowPreviousButton(false)
        made.setBackgroundColor(Color.BLACK)
        made.setFullscreenButtonState(true)
        made.setFullscreenButtonClickListener { wants -> if (!wants) leave() }
        fit(made, fit)
        val shown = Dialog(context, android.R.style.Theme_DeviceDefault_NoActionBar_Fullscreen)
        shown.setContentView(made, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        shown.setOnDismissListener { if (dialog === shown) finish() }
        dialog = shown; view = made
        if (from != null) PlayerView.switchTargetView(exo, from, made) else made.player = exo
        shown.show()
        shown.window?.let { window ->
            window.setLayout(WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT)
            window.attributes = window.attributes.apply { layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES }
            if (Build.VERSION.SDK_INT >= 30) {
                window.insetsController?.let {
                    it.hide(WindowInsets.Type.systemBars())
                    it.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                }
            } else {
                @Suppress("DEPRECATION")
                window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            }
        }
    }

    /** The element's `object-fit`, as the inline controller takes it (D4). */
    fun fit(fit: String) { view?.let { fit(it, fit) } }

    private fun fit(view: PlayerView, fit: String) {
        view.resizeMode = when (fit) {
            "cover" -> AspectRatioFrameLayout.RESIZE_MODE_ZOOM
            "fill" -> AspectRatioFrameLayout.RESIZE_MODE_FILL
            else -> AspectRatioFrameLayout.RESIZE_MODE_FIT
        }
    }

    /** Leave full screen synchronously; [notify] runs [left] to return the player and announce the change. */
    fun leave(notify: Boolean = true) {
        val shown = dialog ?: return
        val made = view ?: return
        dialog = null; view = null
        try {
            if (notify) left(made) else made.player = null
        } finally {
            shown.dismiss()
        }
    }

    private fun finish() {
        val made = view ?: return
        dialog = null; view = null
        left(made)
    }
}
