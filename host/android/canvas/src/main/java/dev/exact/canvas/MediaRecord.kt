package dev.exact.canvas

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.io.File

private val mediaMain = Handler(Looper.getMainLooper())

internal data class MediaAllocation(val id: Int, val slot: Int, val generation: Int, val session: Long) {
    constructor(id: Int, props: JSONObject) : this(id, props.optInt("slot"), props.optInt("generation"), props.optLong("session"))

    fun tell(event: String, payload: String, state: String) = Exact.media(id, slot, generation, session, event, payload, state)
    fun readProps(): String? = Native.backgroundAudio(id, slot, generation, session)?.decodeToString()
}

/**
 * A media node's record (LLP 1042.001 D1), kept across the views and players it borrows, a parked row and a reader
 * made again: the authority a new player starts from.
 */
class MediaRecord internal constructor(internal val allocation: MediaAllocation) {
    val id: Int get() = allocation.id
    /**
     * Where a loan starts: [src] at [positionMs]; HTML's paused flag; whether it ended; [resumes] when Chrome's
     * off-screen rule paused it; the speed and [choices] (the player's own) the controls made; and [applied], the
     * props last applied, so that only a prop that differs from them counts as assigned.
     */
    data class Resume(
        val src: String, val positionMs: Long, val paused: Boolean, val ended: Boolean, val resumes: Boolean,
        val speed: Float, val choices: Any?, val applied: String,
    )
    internal var audioEpoch = 0
    var resume: Resume? = null
        internal set
}

/** Media records by node id, the process's: they outlive a reader made again ([ExactActivity.read]). */
internal object MediaRecords {
    private val records = HashMap<Int, MediaRecord>()
    /** Start a new record when the id names another allocation. */
    fun of(allocation: MediaAllocation): MediaRecord {
        val id = allocation.id
        return records[id]?.takeIf { it.allocation == allocation } ?: MediaRecord(allocation).also { records[id] = it }
    }
    /** The node is gone. */
    fun forget(id: Int) { records.remove(id) }
    fun forget(record: MediaRecord) { if (records[record.id] === record) forget(record.id) }
}

/** The sink follows an audio player across readers. Its redraw callback belongs to the current reader. */
internal class AudioLoan(val record: MediaRecord, val player: VideoPlayer) : MediaSink {
    private val epoch = ++record.audioEpoch
    var redraw: (() -> Unit)? = null
    private var closed = false

    override fun event(name: String, payload: String) = send { record.allocation.tell(name, payload, "") }
    override fun state(json: String) = send { record.allocation.tell("", "", json) }
    override fun firstFrame() = send { redraw?.invoke() }

    private fun send(report: () -> Unit) {
        if (closed) return
        mediaMain.post {
            if (record.audioEpoch == epoch) report()
            BackgroundAudio.ended(this)
        }
    }

    fun close() {
        closed = true
        redraw = null
    }
}

/** Audio loans with no reader. The main thread owns the map and players; the exact thread reads its snapshots. */
internal object BackgroundAudio {
    private class Detached(val loan: AudioLoan, val application: Context) {
        val allocation = loan.record.allocation
        var published = false
        var told: String? = null
    }

    @Volatile private var detached: Map<MediaAllocation, Detached> = emptyMap()
    private var wakes: Wakes? = null
    private val mainTurn = Runnable { runMainTurn() }

    fun detach(loan: AudioLoan, application: Context) {
        loan.redraw = null
        val entry = Detached(loan, application)
        detached = detached + (entry.allocation to entry)
        if (Exact.owner != null) Exact.wake()
        else {
            updateMainThreadWakes()
            reconcileOnMain()
        }
    }

    fun reconcileOnMain() {
        mediaMain.removeCallbacks(mainTurn)
        if (Exact.owner == null && detached.isNotEmpty()) mediaMain.post(mainTurn)
    }

    fun take(allocation: MediaAllocation): AudioLoan? {
        val entry = detached[allocation] ?: return null
        detached = detached - allocation
        updateMainThreadWakes()
        return entry.loan
    }

    /** With owner=0 the reader normally drives the host. Detached playback keeps it running between readers. */
    fun updateMainThreadWakes() {
        val needed = Exact.owner == null && Exact.live && Exact.reader == null && detached.isNotEmpty()
        if (needed == (wakes != null)) return
        wakes?.close()
        wakes = if (needed) Wakes(Looper.getMainLooper().queue) { index ->
            if (index == Wakes.ACQUIRED) Native.acquired(false) else Native.poll()
            reconcileOnMain()
        } else null
        reconcileOnMain()
    }

    private fun runMainTurn() {
        if (Exact.owner != null || detached.isEmpty()) return
        wakes?.let { watching ->
            val due = Native.background(false)
            watching.sync()
            if (due >= 0) mediaMain.postDelayed(mainTurn, kotlin.math.ceil(due).toLong())
            Native.requests()?.let(Requests::detached)
        }
        reconcile { it() }
    }

    fun contains(allocation: MediaAllocation): Boolean = allocation in detached

    /** Reconcile committed props on the exact thread, without asking for layout or paint. */
    fun reconcile(post: (() -> Unit) -> Unit) {
        for (entry in detached.values) {
            val props = entry.allocation.readProps()
            if (entry.published && props == entry.told) continue
            entry.published = true
            entry.told = props
            post { if (detached[entry.allocation] === entry) apply(entry, props) }
        }
    }

    private fun apply(entry: Detached, props: String?) {
        val loan = entry.loan
        if (props == null) {
            take(entry.allocation)
            MediaRecords.forget(loan.record)
            loan.close()
            loan.player.release()
            return
        }
        val json = JSONObject(props)
        resolveMediaSource(json, AssetRoot.of(entry.application))
        loan.player.update(json, 0, 0, null)
        ended(loan)
    }

    fun ended(loan: AudioLoan) {
        val allocation = loan.record.allocation
        if (detached[allocation]?.loan !== loan || loan.player.backgroundPlayback) return
        take(allocation)
        loan.player.stop()
        loan.record.resume = loan.player.snapshot()
        loan.close()
        loan.player.release()
    }
}

internal fun resolveMediaSource(props: JSONObject, assets: File) {
    when (val source = Sources.video(props.optString("src"), assets) { Native.appFile(it)?.let(::File)?.takeIf(Files::regular) }) {
        is Source.Remote -> props.put("uri", source.url)
        is Source.Local -> props.put("uri", Uri.fromFile(source.file).toString())
        is Source.Refused -> props.put("refused", source.why)
    }
}

/** Where a media element's player reports (LLP 1042.001 D3, D7): to exact2, keyed by the element's node. */
interface MediaSink {
    /** An event by HTML's name, with its payload ("" for none). */
    fun event(name: String, payload: String)
    /** The element's observed state, a JSON object: its row of `state.media`. */
    fun state(json: String)
    /** The current source's first frame is in the view. */
    fun firstFrame()
}
