package dev.exact.canvas

import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioManager
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
@LooperMode(LooperMode.Mode.PAUSED)
class SoundOutputTest {
    private val context: Context get() = RuntimeEnvironment.getApplication()

    @Test fun everyManifestSessionHasItsOwnAttributesAndFocusPolicy() {
        val ambient = SoundPolicy.of("ambient")
        assertEquals(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION, ambient.attributes().usage)
        assertEquals(AudioAttributes.CONTENT_TYPE_SONIFICATION, ambient.attributes().contentType)
        assertNull(ambient.focus)
        assertTrue(ambient.obeyRinger)
        assertEquals(ambient, SoundPolicy.of(""))
        for (session in listOf("playback", "playback-mix")) {
            val p = SoundPolicy.of(session)
            assertEquals(AudioAttributes.USAGE_MEDIA, p.attributes().usage)
            assertEquals(AudioAttributes.CONTENT_TYPE_MUSIC, p.attributes().contentType)
            assertEquals(if (session == "playback") "playback" else null, p.focus)
            assertFalse(p.obeyRinger)
        }
    }

    @Test fun theSingleFocusOwnerUsesTheStrongestClaimsAttributes() {
        AudioFocus.application = context
        val manager = context.getSystemService(AudioManager::class.java)
        val shadow = shadowOf(manager)
        val effect = AudioFocus.claim("playback", SoundPolicy.of("playback").attributes()) {}!!
        assertEquals(AudioAttributes.CONTENT_TYPE_MUSIC, shadow.lastAudioFocusRequest.audioFocusRequest.audioAttributes.contentType)
        val capture = AudioFocus.claim("capture") {}!!
        assertEquals(AudioAttributes.CONTENT_TYPE_SPEECH, shadow.lastAudioFocusRequest.audioFocusRequest.audioAttributes.contentType)
        assertEquals(AudioManager.AUDIOFOCUS_GAIN, shadow.lastAudioFocusRequest.audioFocusRequest.focusGain)
        capture.release()
        assertEquals(AudioAttributes.CONTENT_TYPE_MUSIC, shadow.lastAudioFocusRequest.audioFocusRequest.audioAttributes.contentType)
        effect.release()
    }

    private class Backend : SoundBackend {
        var opens = 0
        var broken: Set<Int> = emptySet()
        var closes = 0
        var room = true
        var error = 0.0
        var live = 1.0
        var queued = 0.0
        val ops = ArrayList<JSONObject>()
        override fun open(files: Array<String>, policy: SoundPolicy, decoded: BooleanArray): Long {
            opens++
            decoded.indices.forEach { decoded[it] = it !in broken }
            return 1
        }
        override fun op(handle: Long, op: JSONObject): Boolean {
            if (!room) return false
            ops += op
            return true
        }
        override fun counts(handle: Long) = doubleArrayOf(live, 0.0, 0.0, 0.01, 48000.0, 1.0, error, queued, 0.0)
        override fun close(handle: Long) { closes++ }
    }

    private fun batch(ops: String, reset: Boolean = false) = JSONObject("""{"files":["assets/hit.wav"],"reset":$reset,"ops":[$ops]}""")
    private fun play(id: Int, at: Double = 100.0) = """{"kind":1,"id":$id,"sound":0,"at":$at,"gain":0.5,"rate":1.5,"pan":-0.25}"""

    @Test fun aFailedAssetIsReportedOnceAcrossReopensAndDoesNotRetryTheOutput() {
        val backend = Backend().apply { broken = setOf(1) }
        var at = 100.0
        var state = JSONObject()
        val output = SoundOutput(context, SoundPolicy.of("playback-mix"), backend, { state = JSONObject(it) }, { at })
        fun cues(reset: Boolean = false) = batch(play(1) + "," + play(2).replace("\"sound\":0", "\"sound\":1"), reset)
            .put("files", org.json.JSONArray(listOf("assets/hit.wav", "assets/broken.wav")))
            .put("submitted", at)
        fun failures() = org.robolectric.shadows.ShadowLog.getLogsForTag("exact")
            .filter { it.msg.contains("assets/broken.wav") }
        org.robolectric.shadows.ShadowLog.clear()
        try {
            output.apply(cues())
            assertEquals("running", state.getString("state"))
            assertEquals("1/2", state.getString("decoded"))
            assertEquals(listOf(0, 1), backend.ops.map { it.getInt("sound") })
            assertEquals(1, failures().size)
            at += 2
            output.apply(cues())
            assertEquals(1, backend.opens)
            assertEquals(1, failures().size)
            output.apply(cues(true))
            assertEquals(2, failures().size)
            output.foreground(false)
            output.foreground(true)
            output.apply(cues().put("ops", org.json.JSONArray("[${play(3, at)}]")))
            assertEquals(3, backend.opens)
            assertEquals(2, failures().size)
            assertEquals("running", state.getString("state"))
        } finally { output.destroy() }
    }

    @Test fun cancellingAFutureVoiceClosesTheIdleStreamAndReleasesFocus() {
        val backend = Backend()
        var at = 100.0
        var state = JSONObject()
        val manager = shadowOf(context.getSystemService(AudioManager::class.java))
        val output = SoundOutput(context, SoundPolicy.of("playback"), backend, { state = JSONObject(it) }, { at })
        try {
            output.apply(batch(play(1, 3700.0)))
            val request = manager.lastAudioFocusRequest.audioFocusRequest
            at += 1.0
            output.apply(batch(""))
            assertEquals(0, backend.closes) // A waiting voice still owns a mixer slot.
            output.apply(batch("""{"kind":2,"id":1,"at":3700.0}"""))
            backend.live = 0.0
            backend.queued = 1.0
            at += 1.0
            output.apply(batch(""))
            assertEquals(0, backend.closes)
            backend.queued = 0.0
            output.apply(batch(""))
            at += 0.1
            shadowOf(android.os.Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(50))
            assertEquals(1, backend.closes)
            assertEquals("stopped", state.getString("state"))
            assertSame(request, manager.lastAbandonedAudioFocusRequest)
        } finally { output.destroy() }
    }

    @Test fun releasingOnPermanentLossDoesNotRetakeFocusForRemainingSpeech() {
        val backend = Backend()
        val manager = shadowOf(context.getSystemService(AudioManager::class.java))
        val output = SoundOutput(context, SoundPolicy.of("playback"), backend, {}, { 100.0 })
        val heard = ArrayList<String>()
        var speech: AudioClaim? = null
        var other: AudioClaim? = null
        var later: AudioClaim? = null
        try {
            output.apply(batch(play(1)))
            speech = AudioFocus.claim("transient") { heard += it }
            other = AudioFocus.claim("transient-solo") {}
            assertNotNull(speech)
            val first = manager.lastAudioFocusRequest.audioFocusRequest
            manager.lastAudioFocusRequest.listener.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS)
            shadowOf(android.os.Looper.getMainLooper()).idle()
            assertEquals(1, backend.closes)
            assertEquals(listOf("focus"), heard)
            assertSame(first, manager.lastAudioFocusRequest.audioFocusRequest)
            speech!!.release()
            speech = null
            assertSame(first, manager.lastAudioFocusRequest.audioFocusRequest)
            later = AudioFocus.claim("transient") {}
            assertNotNull(later)
            assertNotSame(first, manager.lastAudioFocusRequest.audioFocusRequest)
        } finally { later?.release(); other?.release(); speech?.release(); output.destroy() }
    }

    @Test fun theFirstPlayLoadsAndBackpressurePreservesPlaysAndStopsInOrder() {
        val backend = Backend()
        var state = JSONObject()
        val output = SoundOutput(context, SoundPolicy.of("playback-mix"), backend, { state = JSONObject(it) }, { 100.0 })
        try {
            output.apply(batch("", true))
            assertEquals(0, backend.opens)
            backend.room = false
            output.apply(batch(play(1) + "," + """{"kind":2,"id":1,"at":100.1}"""))
            assertEquals(1, backend.opens)
            assertTrue(backend.ops.isEmpty())
            backend.room = true
            output.apply(batch("""{"kind":4,"id":2,"at":100.01,"gain":0.75,"rate":null,"pan":0.5,"ramp":0.04}""" + "," + play(2)))
            assertEquals(listOf(1, 2, 4, 1), backend.ops.map { it.getInt("kind") })
            assertEquals(listOf(1, 1, 2, 2), backend.ops.map { it.getInt("id") })
            assertEquals(0.5, backend.ops[0].getDouble("gain"), 0.0)
            assertEquals(1.5, backend.ops[0].getDouble("rate"), 0.0)
            assertEquals(-0.25, backend.ops[0].getDouble("pan"), 0.0)
            assertEquals("running", state.getString("state"))
            assertEquals(0.01, state.getDouble("outputLatency"), 0.0)
        } finally { output.destroy() }
    }

    @Test fun backgroundAndNoisyDiscardPendingVoicesAndTheNextPlayReopens() {
        val backend = Backend()
        var at = 100.0
        val output = SoundOutput(context, SoundPolicy.of("playback-mix"), backend, {}, { at })
        try {
            output.apply(batch(play(1)))
            output.foreground(false)
            assertEquals(1, backend.closes)
            output.apply(batch(play(2, 101.0)))
            assertEquals(1, backend.ops.size)
            at = 102.0
            output.foreground(true)
            output.apply(batch(play(3, 101.0)))
            assertEquals(1, backend.opens)
            output.apply(batch(play(3, 104.0)).put("submitted", 101.0))
            assertEquals(1, backend.opens)
            output.apply(batch(play(4, 102.0)))
            assertEquals(2, backend.opens)
            context.sendBroadcast(Intent(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
            shadowOf(android.os.Looper.getMainLooper()).idle()
            assertEquals(2, backend.closes)
            at = 103.0
            output.apply(batch(play(5, 103.0)))
            assertEquals(listOf(1, 4, 5), backend.ops.map { it.getInt("id") })
        } finally { output.destroy() }
    }

    @Test fun aTransientInterruptionKeepsFocusOnlyUntilItReturnsAndTheNextPlayClaimsAgain() {
        val backend = Backend()
        val manager = shadowOf(context.getSystemService(AudioManager::class.java))
        val output = SoundOutput(context, SoundPolicy.of("playback"), backend, {}, { 100.0 })
        try {
            output.apply(batch(play(1)))
            val first = manager.lastAudioFocusRequest.audioFocusRequest
            val listener = manager.lastAudioFocusRequest.listener
            listener.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
            shadowOf(android.os.Looper.getMainLooper()).idle()
            assertEquals(1, backend.closes)
            output.apply(batch(play(2)))
            assertEquals(1, backend.opens)
            listener.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)
            shadowOf(android.os.Looper.getMainLooper()).idle()
            assertSame(first, manager.lastAbandonedAudioFocusRequest)
            output.apply(batch(play(3)))
            assertEquals(2, backend.opens)
            assertNotSame(first, manager.lastAudioFocusRequest.audioFocusRequest)
        } finally { output.destroy() }
    }

    @Test fun playbackReopensAfterHomeAndAQueuedFocusGainKeepsTheNewStream() {
        val backend = Backend()
        var at = 100.0
        val manager = shadowOf(context.getSystemService(AudioManager::class.java))
        val output = SoundOutput(context, SoundPolicy.of("playback"), backend, {}, { at })
        try {
            output.apply(batch(play(1)))
            val oldListener = manager.lastAudioFocusRequest.listener
            oldListener.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)
            backend.room = false
            output.apply(batch(play(2, 101.0)))
            output.foreground(false)
            at = 104.0
            output.foreground(true)
            backend.room = true
            output.apply(batch(play(3, at)).put("submitted", at))
            shadowOf(android.os.Looper.getMainLooper()).idle()
            output.apply(batch(play(4, at + 1)).put("submitted", at))
            assertEquals(2, backend.opens)
            assertEquals(1, backend.closes)
            assertEquals(listOf(1, 3, 4), backend.ops.map { it.getInt("id") })
        } finally { output.destroy() }
    }

    @Test fun soundDiagnosticsAreOnlyLoggedByDebuggableApps() {
        val flags = context.applicationInfo.flags
        try {
            for (debug in listOf(false, true)) {
                context.applicationInfo.flags = if (debug) flags or android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE
                    else flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE.inv()
                org.robolectric.shadows.ShadowLog.clear()
                val output = SoundOutput(context, SoundPolicy.of("playback-mix"), Backend(), {}, { 100.0 })
                try {
                    output.apply(batch(play(1)).put("submitted", 100.0))
                    output.foreground(false)
                    val logs = org.robolectric.shadows.ShadowLog.getLogsForTag("exact").map { it.msg }
                    if (debug) {
                        assertTrue(logs.any { it.startsWith("sound batch:") && it.contains("submitted=100.0") })
                        assertTrue(logs.any { it.startsWith("sound output:") && it.contains("queued=0.0") })
                        assertTrue(logs.any { it.startsWith("sound foreground:") && it.contains("on=false") })
                    } else assertTrue(logs.isEmpty())
                } finally { output.destroy() }
            }
        } finally { context.applicationInfo.flags = flags }
    }

    @Test fun ambientHonoursTheRingerAndAStreamErrorWaitsASecondBeforeRetrying() {
        val backend = Backend()
        var at = 100.0
        var state = JSONObject()
        val manager = context.getSystemService(AudioManager::class.java)
        manager.ringerMode = AudioManager.RINGER_MODE_SILENT
        val output = SoundOutput(context, SoundPolicy.of("ambient"), backend, { state = JSONObject(it) }, { at })
        try {
            output.apply(batch(play(1)))
            assertEquals(0, backend.opens)
            manager.ringerMode = AudioManager.RINGER_MODE_NORMAL
            output.apply(batch(play(2)))
            assertEquals(1, backend.opens)
            backend.error = -899.0
            output.apply(batch(""))
            assertEquals("failed", state.getString("state"))
            assertEquals(1, backend.closes)
            output.apply(batch(play(3)))
            assertEquals(1, backend.opens)
            at += 1
            backend.error = 0.0
            output.apply(batch(play(4, at)))
            assertEquals(2, backend.opens)
        } finally { output.destroy() }
    }
}
