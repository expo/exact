package dev.exact.video

import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.Timeline
import androidx.media3.common.TrackGroup
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.RendererCapabilities
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.source.TrackGroupArray
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.exoplayer.upstream.DefaultBandwidthMeter
import dev.exact.canvas.MediaSink
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/**
 * The rendition cap (LLP 1042.001 D6) as Media3 applies it: the parameters a player sets for its box, given to a
 * `DefaultTrackSelector` over a ladder, leave the adaptive set this names. The set is what bandwidth may choose from,
 * so these assert it and not the track the bandwidth estimate picks.
 */
@OptIn(UnstableApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class RenditionTest {
    private val made = ArrayList<MediaPlayer>()

    @After fun release() { made.forEach { it.release() }; shadowOf(Looper.getMainLooper()).idle() }

    private object Quiet : MediaSink {
        override fun event(name: String, payload: String) {}
        override fun state(json: String) {}
        override fun firstFrame() {}
    }

    /** The parameters a player sets for a [w]×[h] box with these props; its video's size is not yet known. */
    private fun chosen(w: Int, h: Int, vararg pairs: Pair<String, Any>): TrackSelectionParameters {
        val p = MediaPlayer(RuntimeEnvironment.getApplication()).also { made += it }
        p.bind(Quiet)
        val props = JSONObject().put("src", "assets/a.m3u8").put("uri", "file:///nonexistent/a.m3u8")
            .put("preload", "none").put("fit", "contain")
        for ((k, v) in pairs) props.put(k, v)
        p.update(props, w, h, null)
        return p.snapshot()!!.choices as TrackSelectionParameters
    }

    private fun rung(w: Int, h: Int, bitrate: Int) = Format.Builder()
        .setSampleMimeType(MimeTypes.VIDEO_H264).setCodecs("avc1.640028")
        .setWidth(w).setHeight(h).setAverageBitrate(bitrate).setPeakBitrate(bitrate).build()

    private val ladder = listOf(rung(426, 240, 400_000), rung(640, 360, 800_000), rung(960, 540, 1_600_000),
        rung(1280, 720, 3_000_000), rung(1920, 1080, 6_000_000))

    /** A decoder that plays every rung and switches between them seamlessly. */
    private object Decoder : RendererCapabilities {
        override fun getName() = "decoder"
        override fun getTrackType() = C.TRACK_TYPE_VIDEO
        override fun supportsFormat(format: Format) = RendererCapabilities.create(
            C.FORMAT_HANDLED, RendererCapabilities.ADAPTIVE_SEAMLESS, RendererCapabilities.TUNNELING_NOT_SUPPORTED)
        override fun supportsMixedMimeTypeAdaptation() = RendererCapabilities.ADAPTIVE_NOT_SEAMLESS
    }

    /** The heights of the rungs `DefaultTrackSelector` lets bandwidth choose from under [params]. */
    private fun eligible(params: TrackSelectionParameters, rungs: List<Format>): List<Int> {
        val context = RuntimeEnvironment.getApplication()
        val selector = DefaultTrackSelector(context)
        selector.init({}, DefaultBandwidthMeter.Builder(context).build())
        selector.setParameters(params)
        // A selector chooses by the parameters the player's playback thread has taken, as ExoPlayer hands them over.
        selector.onParametersActivated(params)
        val groups = TrackGroupArray(TrackGroup(*rungs.toTypedArray()))
        val result = selector.selectTracks(arrayOf(Decoder), groups, MediaSource.MediaPeriodId(Any()), Timeline.EMPTY)
        val selection = result.selections[0]!!
        return (0 until selection.length()).map { selection.getFormat(it).height }.sorted()
    }

    @Test fun aLadderThatFillsTheBoxStopsAtTheFirstRungThatFillsIt() =
        assertEquals(listOf(240, 360), eligible(chosen(640, 360), ladder))

    @Test fun aBoxBetweenRungsKeepsTheNextOneUp() =
        assertEquals(listOf(240, 360, 540), eligible(chosen(800, 450), ladder))

    @Test fun aLadderThatNeverFillsTheBoxIsNotFiltered() =
        assertEquals(listOf(240, 360), eligible(chosen(1920, 1080), ladder.take(2)))

    @Test fun preferredMaximumResolutionCapsBelowTheBox() =
        assertEquals(listOf(240, 360), eligible(chosen(1920, 1080, "preferredMaximumResolution" to "640x360"), ladder))

    @Test fun preferredPeakBitRateCapsBelowTheBox() =
        assertEquals(listOf(240, 360, 540), eligible(chosen(1920, 1080, "preferredPeakBitRate" to 2_000_000), ladder))
}
