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
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Bundle
import android.os.CancellationSignal
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.StructPollfd
import android.net.Uri
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import dev.exact.canvas.Extensions
import dev.exact.canvas.Files
import dev.exact.canvas.Native
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.Shadows.shadowOf
import org.robolectric.shadows.ShadowContentResolver
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE, shadows = [Host::class])
class AndroidProviderTest {
    // Robolectric's poll returns zero for a regular file. The provider below returns a seekable file.
    @Implements(Os::class)
    class ReadyFile {
        companion object {
            @JvmStatic @Implementation fun poll(fds: Array<StructPollfd>, timeout: Int): Int = fds.size
        }
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class SessionFiles {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun appRoot(index: Int) =
                File(org.robolectric.RuntimeEnvironment.getApplication().cacheDir, "session-tmp").path
        }
    }

    private val window = Robolectric.buildActivity(Activity::class.java).setup()
    private val activity = window.get()
    private val board = activity.getSystemService(ClipboardManager::class.java)

    init {
        Extensions.front = { activity }
        window.windowFocusChanged(true)
    }

    @After fun leave() { Extensions.front = { null } }

    private open class HeldProvider(val mime: String = "text/plain", val file: File? = null) : ContentProvider() {
        val opened = CountDownLatch(1)
        val cancelled = CountDownLatch(1)
        val release = CountDownLatch(1)
        override fun onCreate() = true
        override fun getType(uri: Uri): String? =
            if (file != null && mime.startsWith("image/")) "image/x-fixture" else mime
        override fun getStreamTypes(uri: Uri, filter: String): Array<String>? {
            return if (file != null && filter == "image/*") arrayOf(mime) else null
        }
        override fun openTypedAssetFile(uri: Uri, mime: String, opts: Bundle?, signal: CancellationSignal?): AssetFileDescriptor? {
            if (uri.lastPathSegment == "cancelled") throw android.os.OperationCanceledException()
            if (file != null && mime != this.mime) throw java.io.FileNotFoundException()
            if (file != null && uri.lastPathSegment != "blocked") file.let { return AssetFileDescriptor(ParcelFileDescriptor.open(it, ParcelFileDescriptor.MODE_READ_ONLY), 0, it.length()) }
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

    private fun providerApp(provider: HeldProvider): ContextWrapper {
        val uri = Uri.parse("content://outside/held")
        val info = ProviderInfo().apply {
            name = "outside.Formats"
            packageName = "outside"
            authority = uri.authority
            applicationInfo = android.content.pm.ApplicationInfo().apply { packageName = "outside"; uid = 10001 }
        }
        shadowOf(activity.packageManager).addOrUpdateProvider(info)
        provider.attachInfo(activity, info)
        ShadowContentResolver.registerProviderInternal("outside", provider)
        return object : ContextWrapper(activity.applicationContext) {
            override fun getContentResolver(): ContentResolver = ContentResolver.wrap(provider)
            override fun checkUriPermission(uri: Uri, pid: Int, uid: Int, flags: Int) = PackageManager.PERMISSION_GRANTED
        }
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE)
    @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aContentImageAndItsCaptionAreOneItemBeforeTheNextString() {
        val jpg = File(activity.cacheDir, "picture.jpg")
        val bitmap = Bitmap.createBitmap(12, 5, Bitmap.Config.ARGB_8888)
        bitmap.eraseColor(android.graphics.Color.GREEN)
        jpg.outputStream().use { assertTrue(bitmap.compress(Bitmap.CompressFormat.JPEG, 90, it)) }
        val provider = HeldProvider("image/jpeg", file = jpg)
        val app = providerApp(provider)
        val clip = ClipData("original text description", arrayOf("text/plain"), ClipData.Item("caption", null, Uri.parse("content://outside/held")))
        clip.addItem(ClipData.Item("next"))
        clip.addItem(ClipData.Item(Uri.parse("content://outside/blocked")))
        val context = ExtensionContext(app, "clipboard")
        val reader = ClipboardModule(context)
        board.setPrimaryClip(clip)
        val result = mutableListOf<JSONObject>()
        reader.later("read", JSONObject(), result::add)
        try {
            assertTrue(provider.opened.await(2, TimeUnit.SECONDS))
            provider.release.countDown()
            awaitReply(result)
        } finally {
            provider.release.countDown()
        }
        val rows = result.single().getJSONArray("exact:answer")
        assertEquals(2, rows.length())
        val picture = rows.getJSONObject(0)
        assertEquals("caption", picture.getString("text/plain"))
        assertEquals("next", rows.getJSONObject(1).getString("text/plain"))
        val path = picture.getString("image/png")
        assertTrue(path.startsWith("app:/tmp/clipboard-") && path.endsWith(".png"))
        val decoded = BitmapFactory.decodeFile(File(context.temporary, path.removePrefix("app:/tmp/")).path)
        assertEquals(12, decoded.width)
        assertEquals(5, decoded.height)
        assertTrue(android.graphics.Color.green(decoded.getPixel(0, 0)) > 250)
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE)
    @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aFocusLossPublishesNoPNGWhileAnotherItemIsPending() {
        val jpg = File(activity.cacheDir, "source.jpg")
        jpg.outputStream().use {
            assertTrue(Bitmap.createBitmap(3, 2, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.JPEG, 90, it))
        }
        val provider = HeldProvider("image/jpeg", file = jpg)
        val context = ExtensionContext(providerApp(provider), "clipboard")
        val reader = ClipboardModule(context)
        val clip = ClipData("images", arrayOf("image/jpeg"), ClipData.Item(Uri.parse("content://outside/blocked")))
        clip.addItem(ClipData.Item(Uri.parse("content://outside/good")))
        board.setPrimaryClip(clip)
        val result = mutableListOf<JSONObject>()
        fun staged() = stagedImage()
        try {
            reader.later("read", JSONObject(), result::add)
            assertTrue(provider.opened.await(2, TimeUnit.SECONDS))
            val until = System.nanoTime() + TimeUnit.SECONDS.toNanos(3)
            while (!staged() && System.nanoTime() < until) Thread.sleep(10)
            assertTrue(staged())
            assertTrue(context.temporary.listFiles().isNullOrEmpty())
            window.windowFocusChanged(false)
            assertEquals("NotAllowedError", result.single().getString("error"))
            assertTrue(context.temporary.listFiles().isNullOrEmpty())
        } finally {
            provider.release.countDown()
        }
    }

    @Test @Config(shadows = [ReadyFile::class])
    fun aProviderCancellationLeavesLaterUrisEligible() {
        val file = File(activity.cacheDir, "plain").apply { writeText("later") }
        val reader = ClipboardModule(ExtensionContext(providerApp(HeldProvider(file = file)), "clipboard"))
        board.setPrimaryClip(ClipData("text", arrayOf("text/plain"), item("cancelled")).apply { addItem(item("good")) })
        val replies = request(reader)
        awaitReply(replies)
        assertEquals("later", replies.single().getJSONArray("exact:answer").getJSONObject(0).getString("text/plain"))
    }

    @Test fun cancellingDuringMetadataLookupStopsTheNextProviderCall() {
        val provider = object : HeldProvider() {
            var streamQueries = 0
            override fun getType(uri: Uri): String {
                opened.countDown()
                release.await(5, TimeUnit.SECONDS)
                return "image/png"
            }
            override fun getStreamTypes(uri: Uri, filter: String): Array<String>? { streamQueries++; return null }
        }
        val app = providerApp(provider)
        val signal = CancellationSignal()
        val worker = java.util.concurrent.Executors.newSingleThreadExecutor()
        try {
            val work = worker.submit {
                try {
                    imageFormats(app, item("held").uri!!, true, Long.MAX_VALUE, { it(signal) }, { it(signal) }) { error("cancelled") }
                } catch (_: android.os.OperationCanceledException) { }
            }
            assertTrue(provider.opened.await(2, TimeUnit.SECONDS))
            signal.cancel()
            provider.release.countDown()
            work.get(2, TimeUnit.SECONDS)
            assertEquals(0, provider.streamQueries)
        } finally {
            provider.release.countDown()
            worker.shutdownNow()
        }
    }

    private inner class Formats : HeldProvider() {
        val fourImages = CountDownLatch(4)
        val metadata = CountDownLatch(1)
        val png = File(activity.cacheDir, "source.png")
        val jpeg = File(activity.cacheDir, "source.jpg")
        val plain = File(activity.cacheDir, "plain").apply { writeText("provider text") }
        init {
            val bitmap = Bitmap.createBitmap(4, 3, Bitmap.Config.ARGB_8888)
            png.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            jpeg.outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 90, it) }
            bitmap.recycle()
        }

        override fun getType(uri: Uri) = when (uri.lastPathSegment) {
            "fifth" -> {
                check(fourImages.await(2, TimeUnit.SECONDS))
                "image/png"
            }
            "metadata" -> {
                metadata.countDown()
                release.await(5, TimeUnit.SECONDS)
                "image/png"
            }
            "raw" -> null
            "url" -> "text/uri-list"
            "stubborn", "short" -> "text/plain"
            "fallback" -> "image/jpeg"
            "plain" -> "text/plain"
            else -> "image/png"
        }
        override fun getStreamTypes(uri: Uri, filter: String) = when {
            uri.lastPathSegment == "deniedText" && filter == "text/plain" -> throw SecurityException()
            uri.lastPathSegment == "dual" && filter == "text/plain" -> arrayOf("text/plain")
            uri.lastPathSegment == "fallback" && filter == "image/*" -> arrayOf("image/png", "image/jpeg")
            else -> null
        }
        private fun descriptor(file: File) =
            AssetFileDescriptor(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY), 0, file.length())

        override fun openFile(uri: Uri, mode: String) = ParcelFileDescriptor.open(png, ParcelFileDescriptor.MODE_READ_ONLY)
        override fun openTypedAssetFile(uri: Uri, mime: String, opts: Bundle?, signal: CancellationSignal?): AssetFileDescriptor? {
            if (uri.lastPathSegment == "stubborn" && mime == "text/plain") {
                fourImages.countDown()
                release.await(5, TimeUnit.SECONDS)
                return null
            }
            if (uri.lastPathSegment in listOf("held", "deniedText") || (uri.lastPathSegment == "dual" && mime == "text/plain")) {
                fourImages.countDown()
                opened.countDown()
                val stop = CountDownLatch(1)
                signal!!.setOnCancelListener { stop.countDown() }
                stop.await(5, TimeUnit.SECONDS)
                return null
            }
            return when {
                uri.lastPathSegment == "short" && mime == "text/plain" ->
                    AssetFileDescriptor(ParcelFileDescriptor.open(plain, ParcelFileDescriptor.MODE_READ_ONLY), 0, MAX_TEXT + 1L)
                uri.lastPathSegment == "url" && mime == "text/*" -> descriptor(plain)
                uri.lastPathSegment == "plain" && mime == "text/plain" -> descriptor(plain)
                uri.lastPathSegment == "fallback" && mime == "image/jpeg" -> descriptor(jpeg)
                uri.lastPathSegment in listOf("png", "dual", "metadata", "fifth") && mime == "image/png" -> descriptor(png)
                uri.lastPathSegment == "raw" && mime == "*/*" -> descriptor(png)
                else -> throw java.io.FileNotFoundException()
            }
        }
    }

    private fun item(name: String) = ClipData.Item(Uri.parse("content://outside/$name"))
    private fun request(reader: ClipboardModule, name: String = "read") =
        mutableListOf<JSONObject>().also { replies -> reader.later(name, JSONObject(), replies::add) }

    private fun stagedImage() = activity.cacheDir.listFiles().orEmpty().any {
        it.isDirectory && it.name.startsWith("clipboard-") && it.listFiles().orEmpty().any { file -> file.extension == "png" }
    }

    private fun readFormats(clip: ClipData, timeout: Boolean = false): JSONObject {
        val provider = Formats()
        val context = ExtensionContext(providerApp(provider), "clipboard")
        board.setPrimaryClip(clip)
        val replies = request(ClipboardModule(context))
        if (timeout) {
            assertTrue(provider.opened.await(2, TimeUnit.SECONDS))
            // Let the ready representation finish before advancing Robolectric's virtual deadline.
            if (clip.getItemAt(0).uri.lastPathSegment == "dual") {
                val until = System.nanoTime() + TimeUnit.SECONDS.toNanos(2)
                while (!stagedImage()
                    && System.nanoTime() < until) Thread.sleep(10)
            }
            shadowOf(Looper.getMainLooper()).idleFor(2, TimeUnit.SECONDS)
        }
        awaitReply(replies)
        return replies.single()
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aStringAppendedToAnImageClipKeepsItsOwnItem() {
        val clip = ClipData(null, arrayOf("image/png"), item("png")).apply { addItem(ClipData.Item("other text")) }
        val rows = readFormats(clip).getJSONArray("exact:answer")
        assertTrue(rows.getJSONObject(0).has("image/png"))
        assertEquals("other text", rows.getJSONObject(1).getString("text/plain"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aStalledTextRepresentationLeavesTheSameItemsPNG() {
        val clip = ClipData(null, arrayOf("text/plain", "image/png"), item("dual"))
        assertTrue(readFormats(clip, timeout = true).getJSONArray("exact:answer").getJSONObject(0).has("image/png"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun anUnavailableTypedPNGLeavesTheNativeJPEG() {
        val clip = ClipData(null, arrayOf("image/jpeg"), item("fallback"))
        assertTrue(readFormats(clip).getJSONArray("exact:answer").getJSONObject(0).has("image/png"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun anImageLabelOnASingleItemCanIdentifyItsUntypedFile() {
        val clip = ClipData(null, arrayOf("image/png"), item("raw"))
        assertTrue(readFormats(clip).getJSONArray("exact:answer").getJSONObject(0).has("image/png"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun anImageRecognizedByItsProviderIsUnreadableWhenItTimesOut() {
        val clip = ClipData(null, arrayOf("text/plain"), item("held"))
        assertEquals("NotReadableError", readFormats(clip, timeout = true).getString("error"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun anImagePasteLeavesCapacityForAnIndependentProviderTextRead() {
        val provider = Formats()
        val reader = ClipboardModule(ExtensionContext(providerApp(provider), "clipboard"))
        board.setPrimaryClip(ClipData(null, arrayOf("text/plain", "image/png"), item("plain")).apply {
            repeat(4) { addItem(item("held")) }
        })
        val paste = request(reader)
        try {
            assertTrue(provider.fourImages.await(2, TimeUnit.SECONDS))
            val second = request(reader)
            awaitReply(second)
            assertEquals("NotReadableError", second.single().getString("error"))
            assertEquals("another clipboard image read is in progress", second.single().getString("message"))
            val caret = request(reader, "readText")
            awaitReply(caret)
            assertEquals("provider text", caret.single().getString("exact:answer"))
            assertTrue(paste.isEmpty())
            board.setPrimaryClip(ClipData.newPlainText(null, "inline"))
            assertEquals("inline", request(reader, "readText").single().getString("exact:answer"))
        } finally {
            window.windowFocusChanged(false)
        }
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aRawPictureStaysReadableWhenItIsTheOnlyUriBesideInlineText() {
        val clip = ClipData(null, arrayOf("image/png"), item("raw")).apply { addItem(ClipData.Item("caption")) }
        val rows = readFormats(clip).getJSONArray("exact:answer")
        assertTrue(rows.getJSONObject(0).has("image/png"))
        assertEquals("caption", rows.getJSONObject(1).getString("text/plain"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aTextProbeCannotChangeTheErrorForAnImageThatTimesOut() {
        val clip = ClipData(null, arrayOf("image/png"), item("deniedText"))
        assertEquals("NotReadableError", readFormats(clip, timeout = true).getString("error"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun theImageLabelAdmitsTheFirstReadEvenWhileItsProviderLooksUpTheType() {
        val provider = Formats()
        val reader = ClipboardModule(ExtensionContext(providerApp(provider), "clipboard"))
        board.setPrimaryClip(ClipData(null, arrayOf("image/png"), item("metadata")))
        try {
            val first = request(reader)
            assertTrue(provider.metadata.await(2, TimeUnit.SECONDS))
            board.setPrimaryClip(ClipData(null, arrayOf("image/png"), item("png")))
            assertEquals("another clipboard image read is in progress", request(reader).single().getString("message"))
            val until = System.nanoTime() + TimeUnit.SECONDS.toNanos(3)
            while (!stagedImage() && System.nanoTime() < until) Thread.sleep(10)
            assertTrue(stagedImage())
            shadowOf(Looper.getMainLooper()).idleFor(2, TimeUnit.SECONDS)
            assertTrue(first.single().getJSONArray("exact:answer").getJSONObject(0).has("image/png"))
        } finally {
            provider.release.countDown()
        }
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aFifthImageCanUseTheDeadlineAfterFourOpensStall() {
        val provider = Formats()
        val reader = ClipboardModule(ExtensionContext(providerApp(provider), "clipboard"))
        board.setPrimaryClip(ClipData(null, arrayOf("image/png"), item("held")).apply {
            repeat(3) { addItem(item("held")) }
            addItem(item("fifth"))
        })
        val replies = request(reader)
        assertTrue(provider.fourImages.await(2, TimeUnit.SECONDS))
        // Metadata for the fifth URI becomes available only after all four workers have opened a file.
        val until = System.nanoTime() + TimeUnit.SECONDS.toNanos(3)
        while (replies.isEmpty() && System.nanoTime() < until) {
            shadowOf(Looper.getMainLooper()).idleFor(50, TimeUnit.MILLISECONDS)
            if (replies.isEmpty()) Thread.sleep(10)
        }
        assertTrue(replies.single().getJSONArray("exact:answer").getJSONObject(0).has("image/png"))
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) @Config(shadows = [SessionFiles::class, ReadyFile::class])
    fun aListOfBlockedTextProvidersLeavesOtherReadsTheirPrimaryWorker() {
        val provider = Formats()
        val reader = ClipboardModule(ExtensionContext(providerApp(provider), "clipboard"))
        board.setPrimaryClip(ClipData(null, arrayOf("text/plain"), item("stubborn")).apply {
            repeat(3) { addItem(item("stubborn")) }
        })
        try {
            request(reader)
            assertTrue(provider.fourImages.await(2, TimeUnit.SECONDS))
            shadowOf(Looper.getMainLooper()).idleFor(2, TimeUnit.SECONDS)
            board.setPrimaryClip(ClipData(null, arrayOf("text/plain"), item("short")))
            val list = request(reader)
            awaitReply(list)
            assertEquals("provider text", list.single().getJSONArray("exact:answer").getJSONObject(0).getString("text/plain"))
            val first = request(reader, "readText")
            awaitReply(first)
            assertEquals("provider text", first.single().getString("exact:answer"))
            board.setPrimaryClip(ClipData(null, arrayOf("text/uri-list"), item("url")))
            val urls = request(reader)
            awaitReply(urls)
            assertEquals(0, urls.single().getJSONArray("exact:answer").length())
        } finally {
            provider.release.countDown()
        }
    }

    private fun awaitReply(replies: List<JSONObject>) {
        val until = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (replies.isEmpty() && System.nanoTime() < until) {
            shadowOf(Looper.getMainLooper()).idle()
            if (replies.isEmpty()) Thread.sleep(10)
        }
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
        assertFalse(readable(app, uri))
        assertFalse(readable(app, uri, image = true))
        val picture = Uri.parse("content://${activity.packageName}.exact.files/app/tmp/picture.png")
        shadowOf(activity.packageManager).addOrUpdateProvider(ProviderInfo().apply {
            packageName = activity.packageName
            name = Files::class.java.name
            applicationInfo = activity.applicationInfo
            authority = picture.authority
        })
        assertTrue(readable(app, picture, image = true))
        assertFalse(readable(app, picture))
        assertEquals(1L, provider.opened.count)
    }
}
