package exact.clipboard

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentProvider
import android.content.ContentResolver
import android.content.ContentValues
import android.content.ContextWrapper
import android.content.pm.PackageManager
import android.content.pm.ProviderInfo
import android.content.res.AssetFileDescriptor
import android.database.Cursor
import android.os.Bundle
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.net.Uri
import dev.exact.canvas.Cause
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.Extensions
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.Shadows.shadowOf
import org.robolectric.shadows.ShadowContentResolver
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [Host::class])
class AndroidReadTest {
    private val window = Robolectric.buildActivity(Activity::class.java).setup()
    private val activity = window.get()
    private val board = activity.getSystemService(ClipboardManager::class.java)
    private val module = ClipboardModule(ExtensionContext(activity.applicationContext, "clipboard"))

    init {
        Extensions.front = { activity }
        window.windowFocusChanged(true)
    }

    @After fun leave() { Extensions.front = { null } }

    private fun read(name: String): JSONObject {
        lateinit var result: JSONObject
        module.later(name, JSONObject()) { result = it }
        shadowOf(Looper.getMainLooper()).idle()
        return result
    }

    @Test fun absenceAndAnEmptyTextItemAreDifferentAndNeitherGrantsAnUnfocusedRead() {
        board.clearPrimaryClip()
        assertEquals("[]", read("read").getJSONArray("exact:answer").toString())
        assertEquals("", read("readText").getString("exact:answer"))
        board.setPrimaryClip(ClipData.newPlainText(null, ""))
        assertEquals("", read("read").getJSONArray("exact:answer").getJSONObject(0).getString("text/plain"))
        window.windowFocusChanged(false)
        assertEquals("NotAllowedError", read("readText").getString("error"))
    }

    @Test fun anImageOffersNoTextAndAnOversizedSecondItemDoesNotHideTheFirstString() {
        board.setPrimaryClip(ClipData(null, arrayOf("image/png"), ClipData.Item(Uri.parse("content://picture/1"))))
        assertEquals("[]", read("read").getJSONArray("exact:answer").toString())
        assertEquals("", read("readText").getString("exact:answer"))
        val first = "line\n\"🍋\""
        board.setPrimaryClip(ClipData.newPlainText(null, first).apply { addItem(ClipData.Item("é".repeat(1 shl 19))) })
        assertEquals(first, read("readText").getString("exact:answer"))
        assertEquals("DataError", read("read").getString("error"))
        board.setPrimaryClip(ClipData.newPlainText(null, "é".repeat((1 shl 19) + 1)))
        assertEquals("DataError", read("readText").getString("error"))
    }

    @Test fun anUnavailableProviderDoesNotHideAnInlineString() {
        val uri = ClipData.Item(Uri.parse("content://outside/missing"))
        val clip = ClipData("text", arrayOf("text/plain"), uri)
        fun items(first: Boolean) = TextItems(clip, first).answer { _, _, _ -> throw java.io.FileNotFoundException() }
        assertEquals("NotReadableError", items(false).getString("error"))
        clip.addItem(ClipData.Item("inline"))
        assertEquals("inline", items(true).getString("exact:answer"))
        assertEquals("inline", items(false).getJSONArray("exact:answer").getJSONObject(0).getString("text/plain"))
    }

    private class HeldProvider : ContentProvider() {
        val opened = CountDownLatch(1)
        val cancelled = CountDownLatch(1)
        val release = CountDownLatch(1)
        override fun onCreate() = true
        override fun getType(uri: Uri) = "text/plain"
        override fun openTypedAssetFile(uri: Uri, mime: String, opts: Bundle?, signal: CancellationSignal?): AssetFileDescriptor? {
            signal!!.setOnCancelListener { cancelled.countDown() }
            opened.countDown()
            release.await(5, TimeUnit.SECONDS)
            return null
        }
        override fun query(u: Uri, p: Array<String>?, s: String?, a: Array<String>?, o: String?): Cursor? = null
        override fun insert(u: Uri, v: ContentValues?): Uri? = null
        override fun update(u: Uri, v: ContentValues?, s: String?, a: Array<String>?) = 0
        override fun delete(u: Uri, s: String?, a: Array<String>?) = 0
    }

    private fun heldRead(name: String, interrupt: (ClipboardModule) -> Unit): List<JSONObject> {
        val provider = HeldProvider()
        val uri = Uri.parse("content://outside/held")
        provider.attachInfo(activity, ProviderInfo().apply { authority = uri.authority })
        ShadowContentResolver.registerProviderInternal("outside", provider)
        val app = object : ContextWrapper(activity.applicationContext) {
            override fun getContentResolver(): ContentResolver = ContentResolver.wrap(provider)
            override fun checkUriPermission(uri: Uri, pid: Int, uid: Int, flags: Int) = PackageManager.PERMISSION_GRANTED
        }
        val reader = ClipboardModule(ExtensionContext(app, "clipboard"))
        board.setPrimaryClip(ClipData("held", arrayOf("text/plain"), ClipData.Item(uri)).apply {
            addItem(ClipData.Item("inline"))
        })
        val replies = mutableListOf<JSONObject>()
        try {
            reader.later(name, JSONObject(), null, Cause("app", request = 42), replies::add)
            assertTrue(provider.opened.await(2, TimeUnit.SECONDS))
            interrupt(reader)
            assertTrue(provider.cancelled.await(1, TimeUnit.SECONDS))
            return replies
        } finally {
            provider.release.countDown()
        }
    }

    @Test fun aTimedOutProviderLeavesTheInlineText() {
        val answer = heldRead("read") { shadowOf(Looper.getMainLooper()).idleFor(2, TimeUnit.SECONDS) }.single()
        assertEquals("inline", answer.getJSONArray("exact:answer").getJSONObject(0).getString("text/plain"))
    }

    @Test fun losingFocusRefusesThePendingRead() {
        val answer = heldRead("readText") { window.windowFocusChanged(false) }.single()
        assertEquals("NotAllowedError", answer.getString("error"))
    }

    @Test fun releasingTheSendCancelsTheProviderWithoutReplying() {
        assertTrue(heldRead("readText") { it.cancel(42) }.isEmpty())
    }

    @Test fun losingFocusWhileAnEmptyClipboardIsBeingClassifiedRefusesTheRead() {
        board.clearPrimaryClip()
        val replies = mutableListOf<JSONObject>()
        Handler(Looper.getMainLooper()).post {
            assertTrue(activity.hasWindowFocus())
            module.later("readText", JSONObject(), replies::add)
        }
        window.windowFocusChanged(false)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("NotAllowedError", replies.single().getString("error"))
    }

    @Test fun evenAGrantedUriCannotBorrowTheAppsOwnProvider() {
        val uri = Uri.parse("content://private.fixture/text")
        val provider = HeldProvider()
        val info = ProviderInfo().apply {
            packageName = activity.packageName
            name = "PrivateProvider"
            applicationInfo = activity.applicationInfo
            authority = uri.authority
        }
        shadowOf(activity.packageManager).addOrUpdateProvider(info)
        provider.attachInfo(activity, info)
        ShadowContentResolver.registerProviderInternal(info.authority, provider)
        val app = object : ContextWrapper(activity.applicationContext) {
            override fun checkUriPermission(uri: Uri, pid: Int, uid: Int, flags: Int) = PackageManager.PERMISSION_GRANTED
        }
        assertNull(uriText(app, uri, 100, true, CancellationSignal(), SystemClock.uptimeMillis() + 1000))
        assertEquals(1L, provider.opened.count)
    }
}
