package exact.nfc

import android.nfc.NdefMessage
import android.nfc.NdefRecord
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A text record as a tag holds it: the status byte, the language and the text (NFC Forum Text RTD 1.0, §3.2.1). */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class RecordsTest {
    private fun text(payload: ByteArray) = NdefRecord(NdefRecord.TNF_WELL_KNOWN, NdefRecord.RTD_TEXT, ByteArray(0), payload)

    private fun fields(r: NdefRecord) = Records.json(r, full = true).let {
        listOf(it.getString("text"), it.getString("lang"), it.getString("encoding"), it.getInt("size"))
    }

    @Test fun anEmptyPayloadIsEmptyText() {
        assertEquals(listOf("", "", "utf-8", 0), fields(text(ByteArray(0))))
    }

    @Test fun aLanguageLongerThanThePayloadEndsWithIt() {
        assertEquals(listOf("", "en", "utf-8", 0), fields(text(byteArrayOf(5) + "en".toByteArray())))
    }

    @Test fun utf8AndUtf16Text() {
        assertEquals(listOf("héllo", "fr", "utf-8", 6), fields(text(byteArrayOf(2) + "fr".toByteArray() + "héllo".toByteArray())))
        assertEquals(listOf("hi", "en", "utf-16be", 4), fields(text(byteArrayOf(0x82.toByte()) + "en".toByteArray() + "hi".toByteArray(Charsets.UTF_16BE))))
    }

    @Test fun aSmartPosterHoldingAnEmptyText() {
        val poster = NdefRecord(NdefRecord.TNF_WELL_KNOWN, NdefRecord.RTD_SMART_POSTER, ByteArray(0),
            NdefMessage(NdefRecord.createUri("https://expo.dev"), text(ByteArray(0))).toByteArray())
        val inner = Records.json(poster, full = true).getJSONArray("records")
        assertEquals("https://expo.dev", inner.getJSONObject(0).getString("text"))
        assertEquals("", inner.getJSONObject(1).getString("text"))
    }
}
