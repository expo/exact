package dev.exact.video

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Matrix
import android.os.Handler
import android.os.Looper
import android.view.ContextThemeWrapper
import android.view.LayoutInflater
import android.view.TextureView
import android.view.View
import android.view.accessibility.AccessibilityManager
import android.widget.FrameLayout
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.VideoSize
import androidx.media3.common.text.CueGroup
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerControlView
import androidx.media3.ui.PlayerView
import androidx.media3.ui.SubtitleView
import dev.exact.canvas.AudioClaim
import dev.exact.canvas.AudioFocus
import dev.exact.canvas.MediaRecord
import dev.exact.canvas.MediaSink
import dev.exact.canvas.VideoPlayer
import org.json.JSONObject
import kotlin.math.roundToLong

/**
 * A `video` element's player on Android (LLP 1042.001): Media3's ExoPlayer, drawn into a `TextureView` without
 * `controls` and into Media3's `PlayerView` with them (D4), keeping HTML's media state ([Projection], D7) for the node
 * it is lent to. Main thread.
 */
@OptIn(UnstableApi::class)
class MediaPlayer(private val context: Context) : VideoPlayer {
    private val selector = DefaultTrackSelector(context)
    private val exo: ExoPlayer = ExoPlayer.Builder(context)
        .setTrackSelector(selector)
        // The process's one focus owner is AudioFocus (D8); a noisy route is route policy, which stays out.
        .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(), false)
        .setHandleAudioBecomingNoisy(false)
        .build()
    private val frame = Frame(context)
    private val texture = TextureView(context).apply { isOpaque = false }
    private val subtitles = SubtitleView(context).apply { setUserDefaultStyle(); setUserDefaultTextSize() }
    private var controls: PlayerView? = null
    /** The element is an `audio`: no picture, and a controller that stays shown (D12). */
    private var audio = false
    private val main = Handler(Looper.getMainLooper())
    private val projection = Projection { name, payload -> sink?.event(name, payload) }
    private val full = FullScreen(context, exo) { left(it) }

    private var sink: MediaSink? = null
    /** The props last applied, as the painter sent them (D1). */
    private var applied: JSONObject? = null
    private var src = ""
    private var prepared = false
    private var generation = 0
    private var fit = "contain"
    private var boxW = 0
    private var boxH = 0
    private var videoW = 0
    private var videoH = 0
    private var muted = false
    private var volume = 1f
    private var speed = 1f
    private var pitchPreserved = true
    private var keepAwake = true
    /** The node handles `timeupdate`: only then does the 4 Hz clock run (D3). */
    private var timed = false
    private var inView = true
    /** Chrome's off-screen rule paused it, so it plays again once seen (LLP 1042 §7 B). */
    private var resumes = false
    /** A play or pause the off-screen rule did not ask for ended the rule for this node. */
    private var overridden = false
    /** This source's first frame is in the view. */
    private var rendered = false
    /** Playback or a seek has begun for this source: with a frame, the poster is gone (D5, LLP 1042 §3). */
    private var begun = false
    private var poster = false
    private val frameReady get() = rendered && begun
    private var error: String? = null
    private var unavailable: String? = null
    private var pendingSeek: Double? = null
    private var seekLanded = false
    private var claim: AudioClaim? = null
    private val focusType: String? = audioSession(context).takeIf { it == "playback" }
    private val access = context.getSystemService(AccessibilityManager::class.java)
    /** exact2's focus is on this controller: it holds the window's focus, so keys reach Media3's own handling (D4). */
    private var held = false
    /** While a finger explores the screen (TalkBack), the controller stays shown, so its buttons can be reached (D11). */
    private val exploring = AccessibilityManager.TouchExplorationStateChangeListener { explore() }

    override val view: View get() = frame
    override val lendable: Boolean get() = controls == null
    override val engaged: Boolean get() = !projection.paused && !eligible()
    override val controller: View? get() = controls

    init {
        frame.addView(texture, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        frame.addView(subtitles)
        exo.setVideoTextureView(texture)
        exo.addListener(listener())
        exo.trackSelectionParameters = exo.trackSelectionParameters.buildUpon().setPreferredTextLanguageAndRoleFlagsToCaptioningManagerSettings(context).build()
        access?.addTouchExplorationStateChangeListener(exploring)
    }

    override fun bind(sink: MediaSink) {
        // A borrowed player carries nothing of the node before (D1), and reports nothing of it to this one.
        this.sink = null
        projection.load(restoring = false, paused = true)
        tick(false)
        releaseFocus()
        exo.playWhenReady = false
        exo.stop()
        exo.clearMediaItems()
        exo.setPlaybackParameters(PlaybackParameters.DEFAULT)
        applied = null; src = ""; prepared = false; rendered = false; begun = false; error = null; unavailable = null
        pendingSeek = null; seekLanded = false; resumes = false; overridden = false; speed = 1f; inView = false; timed = false
        held = false
        videoW = 0; videoH = 0
        this.sink = sink
    }

    override fun update(props: JSONObject, width: Int, height: Int, from: MediaRecord.Resume?) {
        val before = if (from != null) runCatching { JSONObject(from.applied) }.getOrNull() else applied
        generation = props.optInt("generation")
        // Full screen holds the player on the controller it was entered with.
        if (full.on && (controls != null) != props.optBoolean("controls")) full.leave()
        presenter(props.optBoolean("controls"), props.optBoolean("audio"))
        fit = props.optString("fit", "contain")
        full.fit(fit)
        // The controller's full-screen button, unless `controlslist` says `nofullscreen` (D5, D9).
        val button = !audio && "nofullscreen" !in props.optString("controlslist").split(' ')
        controls?.setFullscreenButtonClickListener(if (button) PlayerView.FullscreenButtonClickListener { if (it) fullscreen() } else null)
        poster = props.optBoolean("poster")
        unveil()
        val focused = props.optBoolean("focused")
        if (focused != held) { held = focused; if (focused) focus() }
        if (width != boxW || height != boxH || before?.optString("fit") != fit) { boxW = width; boxH = height; layoutPicture() }

        muted = props.optBoolean("muted")
        volume = props.optDouble("volume", 1.0).toFloat().coerceIn(0f, 1f)
        exo.volume = if (muted) 0f else volume
        exo.repeatMode = if (props.optBoolean("loop")) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
        keepAwake = props.optBoolean("preventsDisplaySleepDuringVideoPlayback", true)
        frame.keepScreenOn = keepAwake && projection.playing
        timed = props.optBoolean("timeupdate")
        tick(exo.isPlaying)
        tracks(props)

        val source = props.optString("src")
        var since = before
        // `load()` and `fastSeek()` are numbered in the props, so a player made again from the record repeats neither.
        // A load starts the source again at 0 with the error cleared, as a changed `src` does (LLP 1042 §3).
        val reload = before != null && changed(before, props, "load")
        if (source != src || applied == null || reload) {
            val restoring = from != null && from.src == source && !reload
            load(props, source, props.optString("uri"), from.takeIf { restoring })
            if (!restoring) since = null
        } else if (changed(before!!, props, "volume") || changed(before, props, "muted")) projection.event("volumechange")
        assigned(since, props)
        // `fastSeek` seeks exactly, as on every host (LLP 1042 §3).
        val fast = props.optString("seek")
        if (fast.isNotEmpty() && (before == null || changed(before, props, "seek"))) fast.substringAfter(' ').toDoubleOrNull()?.takeIf { it.isFinite() }?.let { seek(it.coerceAtLeast(0.0)) }
        // `requestFullscreen` (D9), numbered as they are.
        if (props.has("fullscreen") && before != null && changed(before, props, "fullscreen")) fullscreen()
        // Unmuted while playing: it claims focus now, and a refusal pauses it (D8).
        if (!muted && volume > 0f && !projection.paused && claim == null && !claimFocus()) {
            pause(author = false)
            projection.event("error", "not-allowed")
        }
        applied = props
        pushState()
    }

    /** A source loaded, or the record's prepared again where it was (D7's restoration). */
    private fun load(props: JSONObject, source: String, uri: String, from: MediaRecord.Resume?) {
        src = source
        val restoring = from != null && from.src == source
        prepared = false; rendered = false; error = null; unavailable = null; pendingSeek = null; seekLanded = false
        videoW = 0; videoH = 0
        // A node that had played or moved comes back without its poster.
        begun = restoring && (!from.paused || from.positionMs > 0)
        unveil()
        projection.load(restoring, paused = if (restoring) from.paused else true)
        exo.playWhenReady = false
        if (restoring) {
            resumes = from.resumes
            speed = from.speed
            (from.choices as? TrackSelectionParameters)?.let { exo.trackSelectionParameters = it; tracks(props) }
        }
        if (uri.isEmpty()) {
            exo.clearMediaItems()
            fail("src-not-supported")
            return
        }
        exo.setMediaItem(MediaItem.fromUri(uri), if (restoring) from.positionMs else 0L)
        exo.setPlaybackParameters(PlaybackParameters(speed, if (pitchPreserved) 1f else speed))
        if (restoring && from.ended) projection.end()
        if (props.optString("preload") != "none" || (restoring && !from.paused)) prepare()
        // A node that was playing plays on, claiming focus again; refused, it stays paused (D8).
        if (restoring && !projection.paused) {
            if (muted || volume == 0f || claimFocus()) start()
            else { projection.pause(); projection.event("error", "not-allowed") }
        }
    }

    /** What the author assigned since [before]: only a value that differs counts (LLP 1042 §3, D1). */
    private fun assigned(before: JSONObject?, props: JSONObject) {
        val paused = if (props.isNull("paused")) null else props.optBoolean("paused")
        val fresh = before == null
        // Under the off-screen rule autoplay waits to be seen ([shown]); one that loads out of view starts then.
        if (fresh && props.optBoolean("autoplay") && paused != true) {
            if (eligible(props)) resumes = true else play(Ask.AUTOPLAY)
        }
        if (paused != null && (fresh || changed(before, props, "paused"))) {
            if (paused) pause(author = true) else play(Ask.AUTHOR)
        }
        val time = if (props.isNull("currentTime")) null else props.optDouble("currentTime")
        // A new source starts at 0, so a bound 0 asks for no seek.
        if (time != null && (if (fresh) time > 0 else changed(before, props, "currentTime"))) seek(time)
        rate(props, fresh || changed(before, props, "playbackRate") || changed(before, props, "preservesPitch"))
    }

    private fun changed(before: JSONObject, props: JSONObject, name: String) = before.opt(name)?.toString() != props.opt(name)?.toString()

    private fun rate(props: JSONObject, changed: Boolean) {
        if (!changed) return
        val asked = if (props.isNull("playbackRate")) 1.0 else props.optDouble("playbackRate", 1.0)
        pitchPreserved = props.optBoolean("preservesPitch", true)
        // Out of range: the item plays on at the rate it had; the element's error is not set (D5).
        if (asked < 0.25 || asked > 4.0 || asked.isNaN()) { projection.event("error", "invalid-value"); return }
        val was = speed
        speed = asked.toFloat()
        exo.setPlaybackParameters(PlaybackParameters(speed, if (pitchPreserved) 1f else speed))
        if (was != speed && applied != null) projection.event("ratechange")
    }

    private enum class Ask { AUTOPLAY, RULE, AUTHOR }

    /** Clear the paused flag, as [ask] asks: a play the off-screen rule did not ask for ends the rule. */
    private fun play(ask: Ask) {
        if (ask == Ask.AUTHOR) { overridden = true; resumes = false }
        if (!projection.paused) return
        if (!muted && volume > 0f && !claimFocus()) { projection.event("error", "not-allowed"); return }
        if (projection.ended) { projection.seek(0.0); exo.seekTo(0) }
        projection.play(autoplay = ask != Ask.AUTHOR)
        begin()
        start()
    }

    private fun start() {
        if (!prepared && src.isNotEmpty() && error == null) prepare()
        exo.playWhenReady = true
    }

    private fun pause(author: Boolean) {
        if (author) { overridden = true; resumes = false }
        projection.pause()
        exo.playWhenReady = false
        releaseFocus()
    }

    private fun prepare() { prepared = true; exo.prepare() }

    private fun seek(seconds: Double) {
        begin()
        if (projection.readyState < 1) { pendingSeek = seconds; return }
        projection.seek(seconds)
        exo.seekTo((seconds * 1000).roundToLong().coerceAtLeast(0))
    }

    override fun shown(inView: Boolean) {
        this.inView = inView
        if (!eligible()) return
        if (!inView && !projection.paused) { projection.pause(); exo.playWhenReady = false; releaseFocus(); resumes = true }
        else if (inView && resumes) { resumes = false; play(Ask.RULE) }
    }

    /** Chrome's off-screen rule holds: a muted video its `autoplay` started, `paused` unbound, no other play or pause. */
    private fun eligible(props: JSONObject? = applied) =
        props != null && !overridden && props.optBoolean("autoplay") && props.optBoolean("muted") && props.isNull("paused")

    override fun snapshot(): MediaRecord.Resume? {
        if (src.isEmpty()) return null
        return MediaRecord.Resume(
            src, exo.currentPosition, projection.paused, projection.ended, resumes, speed,
            exo.trackSelectionParameters, applied?.toString() ?: "{}",
        )
    }

    override fun stop() {
        full.leave()
        if (!projection.paused) { val rule = eligible(); projection.pause(); if (rule) resumes = true }
        exo.playWhenReady = false
        releaseFocus()
        tick(false)
    }

    override fun release() {
        access?.removeTouchExplorationStateChangeListener(exploring)
        sink = null
        full.leave()
        tick(false)
        releaseFocus()
        exo.release()
    }

    // The presenter (D4).

    private fun presenter(wanted: Boolean, audio: Boolean) {
        if (controls != null && audio != this.audio) presenter(false, this.audio)
        this.audio = audio
        if (wanted == (controls != null)) return
        if (wanted) {
            val themed = ContextThemeWrapper(context, android.R.style.Theme_DeviceDefault)
            val layout = if (audio) R.layout.exact_audio_controls else R.layout.exact_video_controls
            val made = LayoutInflater.from(themed).inflate(layout, frame, false) as PlayerView
            made.setShowNextButton(false); made.setShowPreviousButton(false)
            // Focusable itself, so it keeps the focus while its controller is hidden and the next key shows it. Media3
            // marks the focus on the controller's buttons; the platform's grey wash over the whole view is not its.
            made.isFocusable = true
            made.defaultFocusHighlightEnabled = false
            made.setControllerVisibilityListener(PlayerView.ControllerVisibilityListener { shown ->
                // Shown again by a key, its play button takes the focus, as Media3 means it to ([focusPlay]). Hidden, its
                // button gives up the focus to the window's first view: the player takes it back.
                if (shown == View.VISIBLE) { if (held && made.hasFocus()) focusPlay(made) }
                else if (held && !made.hasFocus()) made.requestFocus()
            })
            // An `audio` shows no picture, so no captions either (LLP 1042 §8).
            if (audio) made.subtitleView?.visibility = View.GONE
            exo.clearVideoTextureView(texture)
            frame.removeView(texture); frame.removeView(subtitles)
            frame.addView(made, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
            made.player = exo
            controls = made
            unveil()
            explore()
        } else {
            val view = controls ?: return
            view.player = null
            frame.removeView(view)
            controls = null
            frame.addView(texture, 0, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
            frame.addView(subtitles)
            exo.setVideoTextureView(texture)
        }
        layoutPicture()
    }

    /** Full screen (D9): the player goes to a full-screen controller, and comes back when it is left ([left]). */
    private fun fullscreen() {
        if (full.on || audio) return
        val from = controls
        if (from == null) exo.clearVideoTextureView(texture)
        full.enter(from, fit)
        projection.event("fullscreenchange", "true")
        pushState()
    }

    private fun left(view: PlayerView) {
        val inline = controls
        if (inline != null) PlayerView.switchTargetView(exo, view, inline) else { view.player = null; exo.setVideoTextureView(texture) }
        layoutPicture()
        projection.event("fullscreenchange", "false")
        pushState()
    }

    /** exact2's focus reached the controller (D4): it shows, and its play button takes the window's focus ([focusPlay]). */
    private fun focus() {
        val view = controls ?: return
        view.showController()
        focusPlay(view)
    }

    /** The play button of [view]'s controller takes the focus, where Media3 means to put it when the controller shows.
     * A hidden controller's buttons become visible only as its fade starts, a frame later, so Media3's own request
     * finds none: the player holds the focus until then, and the button takes it before that frame is drawn. */
    private fun focusPlay(view: PlayerView) {
        val play = view.findViewById<View>(androidx.media3.ui.R.id.exo_play_pause)
        if (play == null) { if (!view.hasFocus()) view.requestFocus(); return }
        if (play.isFocused || play.requestFocus() || !view.requestFocus()) return
        view.viewTreeObserver.addOnPreDrawListener(object : android.view.ViewTreeObserver.OnPreDrawListener {
            override fun onPreDraw(): Boolean {
                if (!held || !view.hasFocus() || play.isFocused || play.requestFocus()) view.viewTreeObserver.removeOnPreDrawListener(this)
                return true
            }
        })
    }

    /** Media3 hides the controller after a while and on a tap; while touch exploration is on it stays (D11), and an
     * `audio`'s always does (D12). */
    private fun explore() {
        val view = controls ?: return
        val stays = audio || access?.isTouchExplorationEnabled == true
        view.controllerShowTimeoutMs = if (stays) 0 else PlayerControlView.DEFAULT_SHOW_TIMEOUT_MS
        view.controllerHideOnTouch = !stays
        if (stays) view.showController()
    }

    private fun begin() { if (!begun) { begun = true; unveil(); pushState() } }

    /** Under a poster the picture stays unseen until this source's first frame after playback or a seek began, so
     * the poster the painter draws beneath shows through (D5); with none, the first frame shows as it comes. */
    private fun unveil() {
        val hidden = poster && !frameReady
        texture.alpha = if (hidden) 0f else 1f
        controls?.videoSurfaceView?.alpha = if (hidden) 0f else 1f
    }

    /** One rect, the picture `object-fit` makes (D5): the texture's transform, the captions' place, D6's cap. */
    private fun layoutPicture() {
        val rect = Picture.rect(fit, boxW, boxH, videoW, videoH)
        controls?.let { view ->
            // `contain`, `cover` and `fill` are its resize modes; `none` and `scale-down` the texture's transform.
            view.resizeMode = when (fit) {
                "cover" -> AspectRatioFrameLayout.RESIZE_MODE_ZOOM
                "contain" -> AspectRatioFrameLayout.RESIZE_MODE_FIT
                else -> AspectRatioFrameLayout.RESIZE_MODE_FILL
            }
            (view.videoSurfaceView as? TextureView)?.setTransform(if (fit == "none" || fit == "scale-down") matrix(rect) else null)
        }
        if (controls == null) {
            texture.setTransform(matrix(rect))
            frame.picture = rect
        }
        frame.requestLayout()
        val viewport = Picture.viewport(fit, boxW, boxH, videoW, videoH)
        val params = exo.trackSelectionParameters.buildUpon()
        if (viewport != null) params.setViewportSize(viewport.first, viewport.second, false)
        else params.setViewportSizeToPhysicalDisplaySize(true)
        exo.trackSelectionParameters = params.build()
    }

    private fun matrix(rect: Rect): Matrix? {
        if (boxW <= 0 || boxH <= 0) return null
        return Matrix().apply { setScale(rect.width / boxW, rect.height / boxH); postTranslate(rect.left, rect.top) }
    }

    private fun tracks(props: JSONObject) {
        val params = exo.trackSelectionParameters.buildUpon()
        val max = Picture.resolution(props.optString("preferredMaximumResolution").takeIf { it.isNotEmpty() })
        if (max != null) params.setMaxVideoSize(max.first, max.second) else params.clearVideoSizeConstraints()
        val bits = props.optDouble("preferredPeakBitRate", 0.0)
        params.setMaxVideoBitrate(if (bits > 0) bits.toInt() else Int.MAX_VALUE)
        exo.trackSelectionParameters = params.build()
    }

    // What Media3 reports (D7).

    private fun listener() = object : Player.Listener {
        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            // The controller played or paused it.
            if (playWhenReady == !projection.paused) return
            if (playWhenReady) play(Ask.AUTHOR) else pause(author = true)
        }

        override fun onPlaybackStateChanged(state: Int) {
            when (state) {
                Player.STATE_BUFFERING -> projection.buffering()
                Player.STATE_READY -> {
                    metadata()
                    projection.ready()
                    seekLanded = false
                    pendingSeek?.let { pendingSeek = null; seek(it) }
                }
                Player.STATE_ENDED -> if (exo.repeatMode == Player.REPEAT_MODE_OFF) {
                    projection.end()
                    exo.playWhenReady = false
                    releaseFocus()
                }
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            projection.playing(isPlaying)
            frame.keepScreenOn = keepAwake && isPlaying
            tick(isPlaying)
        }

        override fun onTimelineChanged(timeline: androidx.media3.common.Timeline, reason: Int) {
            if (exo.duration != C.TIME_UNSET) metadata()
        }

        override fun onPositionDiscontinuity(old: Player.PositionInfo, new: Player.PositionInfo, reason: Int) {
            when (reason) {
                Player.DISCONTINUITY_REASON_SEEK -> {
                    if (!projection.seeking) projection.seek(new.positionMs / 1000.0)
                    seekLanded = true
                }
                Player.DISCONTINUITY_REASON_AUTO_TRANSITION -> projection.looped()
            }
        }

        override fun onPlaybackParametersChanged(parameters: PlaybackParameters) {
            // The controller's speed menu.
            if (parameters.speed != speed) { speed = parameters.speed; projection.event("ratechange") }
        }

        override fun onVideoSizeChanged(size: VideoSize) {
            if (size.width <= 0 || size.height <= 0) return
            videoW = (size.width * size.pixelWidthHeightRatio).toInt(); videoH = size.height
            layoutPicture()
        }

        override fun onRenderedFirstFrame() {
            rendered = true
            unveil()
            sink?.firstFrame()
        }

        override fun onCues(cueGroup: CueGroup) { subtitles.setCues(cueGroup.cues) }

        override fun onPlayerError(e: PlaybackException) {
            if (e.errorCode == PlaybackException.ERROR_CODE_DECODER_INIT_FAILED) {
                // Not a media error: the device's decoders are all held (D1).
                unavailable = "no decoder free"
                pushState()
                return
            }
            fail(when {
                projection.readyState < 1 -> "src-not-supported"
                e.errorCode in 2000..2999 -> "network"
                else -> "decode"
            })
        }

        override fun onEvents(player: Player, events: Player.Events) {
            // A seek within what is buffered lands without leaving STATE_READY.
            if (seekLanded && projection.seeking && exo.playbackState == Player.STATE_READY) { seekLanded = false; projection.seekDone() }
            pushState()
        }
    }

    private fun metadata() {
        val ms = exo.duration
        projection.metadata(if (ms == C.TIME_UNSET) null else ms / 1000.0)
    }

    private fun fail(code: String) {
        error = code
        projection.event("error", code)
        pushState()
    }

    // Time, state and focus.

    private val ticker = object : Runnable {
        override fun run() {
            projection.event("timeupdate", currentTime().toString())
            pushState()
            main.postDelayed(this, 250)
        }
    }
    private var ticking = false

    private fun tick(playing: Boolean) {
        val on = playing && timed
        if (on == ticking) return
        ticking = on
        if (on) main.postDelayed(ticker, 250) else main.removeCallbacks(ticker)
    }

    private fun currentTime() = projection.seekTarget ?: (exo.currentPosition / 1000.0)

    private fun pushState() {
        val sink = sink ?: return
        val format = exo.videoFormat
        val state = JSONObject()
            .put("currentTime", currentTime())
            .put("duration", projection.duration ?: JSONObject.NULL)
            .put("paused", projection.paused)
            .put("playing", projection.playing)
            .put("playbackRate", speed.toDouble())
            .put("muted", muted)
            .put("volume", volume.toDouble())
            .put("readyState", projection.readyState)
            .put("seeking", projection.seeking)
            .put("ended", projection.ended)
            .put("videoWidth", videoW)
            .put("videoHeight", videoH)
            .put("renderer", if (unavailable != null) "none" else if (controls != null) "PlayerView" else "TextureView")
            .put("generation", generation)
            .put("frameReady", frameReady)
            .put("fullscreen", full.on)
            .put("error", error ?: JSONObject.NULL)
            .put("rendition", format?.let { JSONObject().put("width", it.width).put("height", it.height).put("bitrate", it.bitrate) } ?: JSONObject.NULL)
        unavailable?.let { state.put("unavailable", it) }
        sink.state(state.toString())
    }

    /** A claim on the process's focus while it plays with sound under `audio_session: "playback"` (D8). */
    private fun claimFocus(): Boolean {
        val type = focusType ?: return true
        if (claim != null) return true
        claim = AudioFocus.claim(type, android.media.AudioAttributes.Builder()
            .setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MOVIE).build(),
            pauseWhenDucked = false) { change ->
            // Any loss pauses it and releases the claim; focus regained does not resume it (D8).
            if (change == "focus" || change == "focus-transient") pause(author = false)
        }
        return claim != null
    }

    private fun releaseFocus() { claim?.release(); claim = null }

    /** The picture's frame: the texture fills it and the captions sit on the picture's rect (D5). */
    @SuppressLint("ViewConstructor")
    private class Frame(context: Context) : FrameLayout(context) {
        var picture: Rect? = null
        override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
            super.onLayout(changed, left, top, right, bottom)
            val rect = picture ?: return
            for (i in 0 until childCount) {
                val child = getChildAt(i) as? SubtitleView ?: continue
                val l = rect.left.coerceAtLeast(0f).toInt(); val t = rect.top.coerceAtLeast(0f).toInt()
                val r = (rect.left + rect.width).coerceAtMost(width.toFloat()).toInt()
                val b = (rect.top + rect.height).coerceAtMost(height.toFloat()).toInt()
                child.measure(MeasureSpec.makeMeasureSpec(r - l, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(b - t, MeasureSpec.EXACTLY))
                child.layout(l, t, r, b)
            }
        }
    }

    private companion object {
        fun audioSession(context: Context): String = runCatching {
            context.packageManager.getApplicationInfo(context.packageName, PackageManager.GET_META_DATA)
                .metaData?.getString("dev.exact.audioSession")
        }.getOrNull() ?: "ambient"
    }
}
