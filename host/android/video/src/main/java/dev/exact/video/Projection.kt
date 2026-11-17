package dev.exact.video

/**
 * HTML's media state for one node, and the events each change of it sends (LLP 1042.001 D7): a bounded projection of
 * the media element, fed by the player's callbacks. Media3's state is not the element's: the paused flag is what was
 * asked, `playing` what the engine does. [send] takes each event by its name and payload.
 */
internal class Projection(private val send: (String, String) -> Unit) {
    /** HTML's paused flag. */
    var paused = true
        private set
    var playing = false
        private set
    /** HTML's ready state: 0 before metadata, 1 with it, 2 buffering after it, 4 ready. */
    var readyState = 0
        private set
    var seeking = false
        private set
    var ended = false
        private set
    /** Seconds, or null while unknown or indefinite. */
    var duration: Double? = null
        private set
    /** The target of the seek under way, which `currentTime` and `timeupdate` report (LLP 1042 §3). */
    var seekTarget: Double? = null
        private set
    /** A loan starting from its record (D7's restoration): no load or seek events until its first ready state. */
    var restoring = false
        private set
    /** Ready was reached on this item, so a later buffering is a stall. */
    private var reached = false

    /** A new item: a source loaded, or (with [restoring]) a record's item prepared again at its position. */
    fun load(restoring: Boolean, paused: Boolean) {
        this.paused = paused
        this.restoring = restoring
        playing = false; seeking = false; ended = false; seekTarget = null
        duration = null
        // A restored node keeps the state it had: a stall before its first ready is a stall.
        reached = restoring
        readyState = if (restoring) 4 else 0
    }

    /** The duration or the tracks are known; again when the duration changes. */
    fun metadata(seconds: Double?) {
        val finite = seconds?.takeIf { it.isFinite() && it >= 0 }
        if (readyState >= 1 && !restoring) {
            if (finite != duration) { duration = finite; finite?.let { send("durationchange", it.toString()) } }
            return
        }
        duration = finite
        if (restoring) return
        readyState = 1
        finite?.let { send("durationchange", it.toString()) }
        send("loadedmetadata", "")
    }

    /** Media3's `STATE_READY`. */
    fun ready() {
        val was = readyState
        readyState = 4
        if (restoring) { restoring = false; reached = true; if (was < 3) send("canplay", ""); return }
        if (seeking) seekDone()
        if (was < 3) send("canplay", "")
        reached = true
    }

    /** Media3's `STATE_BUFFERING`: before the first ready it is the load, after it a stall. */
    fun buffering() {
        // A paused node being restored shows the state it had: nothing is waiting on the data.
        if (!reached || readyState < 3 || (restoring && paused)) return
        readyState = 2
        if (!paused && !seeking) send("waiting", "")
    }

    /** Clear the paused flag: false when it was clear. [autoplay]: the attribute or the off-screen rule asked, which
     * waits for data without a `waiting`. */
    fun play(autoplay: Boolean): Boolean {
        if (!paused) return false
        paused = false
        send("play", "")
        if (readyState < 3 && !autoplay && !restoring) send("waiting", "")
        return true
    }

    /** Set the paused flag: false when it was set. */
    fun pause(): Boolean {
        if (paused) return false
        paused = true
        send("pause", "")
        return true
    }

    fun playing(on: Boolean) {
        if (on == playing) return
        playing = on
        if (on) { ended = false; send("playing", "") }
    }

    /** A seek begins: the author's, the controls' or a play from the end. */
    fun seek(target: Double?) {
        ended = false
        seekTarget = target
        if (restoring || seeking) return
        seeking = true
        send("seeking", "")
    }

    /** The seek's position is known and the player is ready. */
    fun seekDone() {
        seekTarget = null
        if (!seeking) return
        seeking = false
        send("seeked", "")
    }

    /** A loop's automatic transition to the start, as Chrome reports one (LLP 1042 §3). */
    fun looped() {
        send("seeking", ""); send("waiting", ""); send("seeked", ""); send("canplay", "")
        if (!paused) send("playing", "")
    }

    /** The item ended without looping: `pause`, then `ended`, as the element does. */
    fun end() {
        if (ended) return
        ended = true
        playing = false
        if (!paused) { paused = true; send("pause", "") }
        send("ended", "")
    }

    fun event(name: String, payload: String = "") = send(name, payload)
}
