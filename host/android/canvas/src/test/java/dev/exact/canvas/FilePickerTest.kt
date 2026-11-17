package dev.exact.canvas

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import org.json.JSONArray
import org.junit.Assert.*
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.File
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [33], manifest = Config.NONE, shadows = [FilePickerTest.Host::class])
class FilePickerTest {
    @Before fun begin() {
        Extensions.started()
        // Robolectric replaces the app directories between tests but reuses Kotlin statics.
        FilePicker::class.java.getDeclaredField("imports").apply { isAccessible = true }.set(null, null)
    }
    @After fun leave() { Exact.reader = null; Exact.owner = null; Extensions.paused(); Extensions.stopped(false); Extensions.front = { Exact.reader?.get() }; Host.stop() }

    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            private val running = ThreadLocal<Boolean>()
            val answers = ArrayList<String?>()
            @Volatile var attempted = false
            fun start() { running.set(true); answers.clear(); attempted = false }
            fun stop() { running.remove(); answers.clear() }
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun answer(id: Int, value: String?) {
                if (running.get() == true) answers.add(value)
                attempted = true
            }
        }
    }

    private fun await(done: () -> Boolean) {
        val until = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
        while (!done() && System.nanoTime() < until) {
            shadowOf(Looper.getMainLooper()).idle()
            if (!done()) Thread.sleep(10)
        }
        assertTrue("picker did not complete", done())
    }


    @Test fun requestKindsAreUnique() {
        val kinds = Requests::class.java.declaredFields.filter {
            java.lang.reflect.Modifier.isStatic(it.modifiers) && it.type == Int::class.javaPrimitiveType
        }.associate { it.isAccessible = true; it.name to it.getInt(null) }
        assertEquals(kinds.toString(), kinds.size, kinds.values.toSet().size)
    }

    @Test fun fileRequestsUseKind18() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        Requests(activity).perform(intArrayOf(18, 377, 3, 42, 1, 0))
        val started = shadowOf(activity).nextStartedActivityForResult
        assertNotNull("kind 18 must open the file picker", started)
        assertEquals(Intent.ACTION_OPEN_DOCUMENT, started.intent.action)
        assertTrue(started.intent.getBooleanExtra(Intent.EXTRA_ALLOW_MULTIPLE, false))
    }

    @Test @Config(sdk = [31]) fun backportedPhotoPickerSupportsMultipleSelection() {
        val intent = FilePicker.intent(listOf("image/*"), true, true)
        assertEquals(MediaStore.ACTION_PICK_IMAGES, intent.action)
        assertEquals(100, intent.getIntExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, 0))
    }

    @Test fun acceptChoosesThePlatformPickerAndItsMimeFilters() {
        assertEquals(listOf("image/jpeg", "image/png"), FilePicker.mimeTypes(listOf(".JPG", " image/jpeg ", ".png")))
        val images = FilePicker.intent(listOf("image/*"), true, true)
        assertEquals(MediaStore.ACTION_PICK_IMAGES, images.action)
        assertEquals("image/*", images.type)
        assertTrue(images.getIntExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, 0) > 1)
        for ((accept, offered) in listOf(
            listOf("image/jpeg") to listOf("image/jpeg", "image/heic", "image/heif"),
            listOf(".jpg") to listOf("image/jpeg", "image/heic", "image/heif"),
            listOf(".jpeg") to listOf("image/jpeg", "image/heic", "image/heif"),
            listOf("image/png") to listOf("image/png"),
            listOf("video/mp4") to listOf("video/mp4"),
            listOf("image/heic") to listOf("image/heic")
        )) {
            val intent = FilePicker.intent(accept, false, true)
            assertEquals(accept.toString(), Intent.ACTION_OPEN_DOCUMENT, intent.action)
            assertEquals(offered.singleOrNull() ?: "*/*", intent.type)
            if (offered.size > 1) assertArrayEquals(offered.toTypedArray(), intent.getStringArrayExtra(Intent.EXTRA_MIME_TYPES))
            else assertNull(intent.getStringArrayExtra(Intent.EXTRA_MIME_TYPES))
        }
        assertEquals("video/*", FilePicker.intent(listOf("video/*"), false, true).type)
        assertNull(FilePicker.intent(listOf("image/*", "video/*"), false, true).type)
        val documents = FilePicker.intent(listOf("image/png", "application/pdf"), true, true)
        assertEquals(Intent.ACTION_OPEN_DOCUMENT, documents.action)
        assertEquals("*/*", documents.type)
        assertArrayEquals(arrayOf("image/png", "application/pdf"), documents.getStringArrayExtra(Intent.EXTRA_MIME_TYPES))
        assertTrue(documents.hasCategory(Intent.CATEGORY_OPENABLE))
        assertTrue(documents.getBooleanExtra(Intent.EXTRA_ALLOW_MULTIPLE, false))
        for (types in listOf(emptyList(), listOf("*/*"))) {
            assertEquals(Intent.ACTION_OPEN_DOCUMENT, FilePicker.intent(types, false, true).action)
        }
        assertEquals(Intent.ACTION_OPEN_DOCUMENT, FilePicker.intent(listOf("image/*"), false, false).action)
    }

    @Test fun concreteMimeListsIncludeConvertibleSources() {
        for (types in listOf(listOf("image/jpeg", "image/png"), listOf("video/mp4", "video/webm"), listOf("image/jpeg", "video/*"))) {
            val intent = FilePicker.intent(types, true, true)
            assertEquals(Intent.ACTION_OPEN_DOCUMENT, intent.action)
            val offered = if (types.any { it.startsWith("image/") }) types + listOf("image/heic", "image/heif") else types
            assertArrayEquals(offered.toTypedArray(), intent.getStringArrayExtra(Intent.EXTRA_MIME_TYPES))
        }
    }

    @Test fun unknownExtensionsUseUnrestrictedDocuments() {
        for (accept in listOf(listOf("image/jpeg", ".board"), listOf(".jpg", "application/pdf", ".unknown-exact"))) {
            val mixed = FilePicker.intent(accept, false, true)
            assertEquals(Intent.ACTION_OPEN_DOCUMENT, mixed.action)
            assertEquals("*/*", mixed.type)
            assertNull(mixed.getStringArrayExtra(Intent.EXTRA_MIME_TYPES))
        }
    }

    class FailedActivity : ExactActivity() {
        var launches = 0
        override fun startActivityForResult(intent: Intent, requestCode: Int) {
            launches++
            throw IllegalStateException("activity state saved")
        }
    }

    @Test fun unrelatedLaunchFailuresRefuseWithoutFallingBack() {
        val activity = Robolectric.buildActivity(FailedActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val answers = ArrayList<String?>()
        FilePicker.show(activity, listOf("image/*"), false) { answers.add(it) }
        assertEquals(listOf("file picker: UnknownError"), answers)
        assertEquals(1, activity.launches)
    }

    @Test fun resultsCopyEveryUriAfterTheReaderIsReplacedAndSettleOnlyOnce() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val first = Uri.parse("content://picker/one")
        val second = Uri.parse("content://picker/two")
        val resolver = activity.applicationContext.contentResolver
        shadowOf(resolver).registerInputStream(first, "first".byteInputStream())
        shadowOf(resolver).registerInputStream(second, "second".byteInputStream())
        val done = CountDownLatch(1)
        val answers = ArrayList<String?>()
        FilePicker.show(activity, listOf("image/*"), true) { answers.add(it); done.countDown() }
        val started = shadowOf(activity).nextStartedActivityForResult
        assertEquals(MediaStore.ACTION_PICK_IMAGES, started.intent.action)
        Extensions.paused()
        Exact.reader = WeakReference(Robolectric.buildActivity(ExactActivity::class.java).get())
        val result = Intent().apply {
            clipData = ClipData.newRawUri("photos", first).apply { addItem(ClipData.Item(second)) }
        }
        Extensions.activityResult(started.requestCode, Activity.RESULT_OK, result)
        Extensions.activityResult(started.requestCode, Activity.RESULT_OK, result)
        await { done.count == 0L }
        assertEquals(1, answers.size)
        val paths = JSONArray(answers.single())
        val files = (0 until paths.length()).map { File(paths.getJSONObject(it).getString("path")) }
        assertEquals(listOf("first", "second"), files.map { it.readText() })
        assertTrue(files.all { it.canonicalPath.startsWith(activity.cacheDir.canonicalPath + "/") })
        files.forEach { it.delete(); it.parentFile?.delete() }
        assertEquals(listOf(first), FilePicker.chosen(Activity.RESULT_OK, result, false))
    }

    @Test @Config(sdk = [29]) fun olderAndroidUsesDocumentsForPhotos() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val answers = ArrayList<String?>()
        FilePicker.show(activity, listOf("image/*"), true) { answers.add(it) }
        val started = shadowOf(activity).nextStartedActivityForResult
        assertEquals(Intent.ACTION_OPEN_DOCUMENT, started.intent.action)
        assertEquals("image/*", started.intent.type)
        assertTrue(started.intent.getBooleanExtra(Intent.EXTRA_ALLOW_MULTIPLE, false))
        Extensions.activityResult(started.requestCode, Activity.RESULT_CANCELED, null)
        assertEquals(listOf<String?>(null), answers)
    }

    @Test fun cancellationEmptyResultsAndLaunchRefusalsSettle() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        for ((code, data) in listOf(Activity.RESULT_CANCELED to Intent(), Activity.RESULT_OK to null, Activity.RESULT_OK to Intent())) {
            val answers = ArrayList<String?>()
            FilePicker.show(activity, listOf("image/*"), false) { answers.add(it) }
            val started = shadowOf(activity).nextStartedActivityForResult
            Extensions.activityResult(started.requestCode, code, data)
            assertEquals(listOf<String?>(null), answers)
        }
        Exact.reader = null
        var refused: String? = null
        FilePicker.show(activity, emptyList(), false) { refused = it }
        assertEquals("file picker: InvalidStateError", refused)
    }

    class MissingPickerActivity : ExactActivity() {
        override fun startActivityForResult(intent: Intent, requestCode: Int) {
            throw android.content.ActivityNotFoundException("no picker installed")
        }
    }

    @Test fun aMissingDocumentActivityRefusesWhileOtherCallersStillCancel() {
        val activity = Robolectric.buildActivity(MissingPickerActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val answers = ArrayList<String?>()
        FilePicker.show(activity, emptyList(), false) { answers.add(it) }
        assertEquals(listOf("file picker: NotFoundError"), answers)
        var cancelled = false
        ExtensionContext(activity.applicationContext, "test").startActivityForResult(Intent(Intent.ACTION_VIEW)) { code, _ ->
            cancelled = code == Activity.RESULT_CANCELED
        }
        assertTrue(cancelled)
    }

    class NoPhotosActivity : ExactActivity() {
        val actions = ArrayList<String?>()
        override fun startActivityForResult(intent: Intent, requestCode: Int) {
            actions.add(intent.action)
            if (intent.action == MediaStore.ACTION_PICK_IMAGES) throw android.content.ActivityNotFoundException()
            super.startActivityForResult(intent, requestCode)
        }
    }

    @Test fun aMissingPhotoPickerFallsBackOnceAndBackStillCancels() {
        val activity = Robolectric.buildActivity(NoPhotosActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val answers = ArrayList<String?>()
        FilePicker.show(activity, listOf("image/*"), false) { answers.add(it) }
        assertEquals(listOf(MediaStore.ACTION_PICK_IMAGES, Intent.ACTION_OPEN_DOCUMENT), activity.actions)
        assertEquals(listOf(FilePicker.FALLBACK), answers)
        answers.clear()
        val document = shadowOf(activity).nextStartedActivityForResult
        Extensions.activityResult(document.requestCode, Activity.RESULT_CANCELED, null)
        assertEquals(listOf<String?>(null), answers)
    }

    @Test fun aCopiedResultReachesTheNativeHostWithoutAnOwner() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Exact.owner = null
        Host.start()
        Extensions.resumed()
        val uri = Uri.parse("content://picker/native")
        shadowOf(activity.contentResolver).registerInputStream(uri, "bytes".byteInputStream())
        FilePicker.show(activity, emptyList(), false) { Exact.answer(377, it) }
        val started = shadowOf(activity).nextStartedActivityForResult
        Extensions.activityResult(started.requestCode, Activity.RESULT_OK, Intent().setData(uri))
        await { Host.attempted }
        assertEquals(1, Host.answers.size)
        val file = File(JSONArray(Host.answers.single()).getJSONObject(0).getString("path"))
        assertEquals("bytes", file.readText())
        file.delete(); file.parentFile!!.delete()
    }

    class PhotoProvider : android.content.ContentProvider() {
        override fun onCreate() = true
        override fun getType(uri: Uri) = when (uri.lastPathSegment) {
            "scan", "long", "unicode" -> "image/jpeg"
            "doc" -> "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            "heic", "heic-scan" -> "image/heic"
            "jpe" -> "image/jpeg"
            else -> "image/png"
        }
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?) =
            android.database.MatrixCursor(arrayOf(android.provider.OpenableColumns.DISPLAY_NAME)).apply {
                addRow(arrayOf(when (uri.lastPathSegment) {
                    "long" -> "a".repeat(255)
                    "unicode" -> "写真🌅".repeat(100)
                    "unnamed" -> "photo"
                    "scan" -> "scan.v2"
                    "doc" -> "report"
                    "heic" -> "photo.HEIC"
                    "heic-scan" -> "scan.v2"
                    "jpe" -> "photo.jpe"
                    else -> "../photo.png"
                }))
            }
        override fun insert(uri: Uri, values: android.content.ContentValues?) = null
        override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
        override fun update(uri: Uri, values: android.content.ContentValues?, selection: String?, args: Array<out String>?) = 0
    }

    @Test fun stagingNamesAreIndependentOfDisplayNames() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val resolver = activity.contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("names", PhotoProvider())
        val uris = listOf("unnamed", "named", "same").map { Uri.parse("content://names/$it") }
        uris.forEach { shadowOf(resolver).registerInputStream(it, "png".byteInputStream()) }
        val cache = java.nio.file.Files.createTempDirectory("exact-picker-names").toFile()
        try {
            val paths = JSONArray(FilePicker.copy(resolver, cache, uris))
            val files = (0 until paths.length()).map { File(paths.getJSONObject(it).getString("path")) }
            assertTrue(files.all { it.name.startsWith("source-") && it.name.length < 100 })
            assertEquals(listOf("photo", "photo.png", "photo.png"),
                (0 until paths.length()).map { paths.getJSONObject(it).getString("name") })
            assertEquals(3, files.map { it.parent }.distinct().size)
            assertTrue(files.all { it.canonicalPath.startsWith(cache.canonicalPath + "/") })
        } finally { cache.deleteRecursively() }
    }

    @Test fun providerMetadataTravelsWithTheStagedPath() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val resolver = activity.contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("metadata", PhotoProvider())
        val uris = listOf("unnamed", "scan", "doc").map { Uri.parse("content://metadata/$it") }
        uris.forEach { shadowOf(resolver).registerInputStream(it, "bytes".byteInputStream()) }
        val cache = java.nio.file.Files.createTempDirectory("exact-picker-metadata").toFile()
        try {
            val result = JSONArray(FilePicker.copy(resolver, cache, uris))
            val items = (0 until result.length()).map { result.getJSONObject(it) }
            assertEquals(listOf("photo", "scan.v2", "report"), items.map { it.getString("name") })
            assertEquals(uris.map { resolver.getType(it) }, items.map { it.getString("type") })
            assertEquals(listOf("bytes", "bytes", "bytes"), items.map { File(it.getString("path")).readText() })
        } finally { cache.deleteRecursively() }
    }

    private fun copiesLongDisplayName(segment: String, expected: String) {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val resolver = activity.contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("long-names", PhotoProvider())
        val uri = Uri.parse("content://long-names/$segment")
        shadowOf(resolver).registerInputStream(uri, "jpeg bytes".byteInputStream())
        val cache = java.nio.file.Files.createTempDirectory("exact-picker-long-name").toFile()
        try {
            val answer = FilePicker.copy(resolver, cache, listOf(uri))
            assertTrue(answer, answer.startsWith("["))
            val picked = JSONArray(answer).getJSONObject(0)
            assertEquals(expected, picked.getString("name"))
            assertEquals("image/jpeg", picked.getString("type"))
            val file = File(picked.getString("path"))
            assertEquals("jpeg bytes", file.readText())
            assertTrue(file.name.toByteArray(Charsets.UTF_8).size < 100)
        } finally { cache.deleteRecursively() }
    }

    @Test fun anExtensionless255ByteNameStillImports() = copiesLongDisplayName("long", "a".repeat(255))

    @Test fun aLongUnicodeDisplayNameStillImports() = copiesLongDisplayName("unicode", "写真🌅".repeat(100))

    @Implements(android.content.ContentResolver::class)
    class QuotaResolver : org.robolectric.shadows.ShadowContentResolver() {
        var attempts = 0
        fun seed(uri: Uri, flags: Int) = super.takePersistableUriPermission(uri, flags)
        @Implementation override fun takePersistableUriPermission(uri: Uri, flags: Int) {
            attempts++
            // Robolectric has no quota. Model Android's eviction when a new grant exceeds it.
            if (persistedUriPermissions.size == 512) {
                val oldest = persistedUriPermissions.first()
                super.releasePersistableUriPermission(oldest.uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            }
            super.takePersistableUriPermission(uri, flags)
        }
    }

    @Test @Config(shadows = [QuotaResolver::class]) fun importingLeavesPersistedGrantsAtCapacityUntouched() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val resolver = activity.applicationContext.contentResolver
        val shadow = org.robolectric.shadow.api.Shadow.extract<QuotaResolver>(resolver)
        repeat(512) { shadow.seed(Uri.parse("content://picker/existing-$it"),
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION) }
        val before = resolver.persistedUriPermissions.map { Triple(it.uri, it.isReadPermission, it.isWritePermission) }
        val uri = Uri.parse("content://picker/import")
        shadow.registerInputStream(uri, "bytes".byteInputStream())
        var answer: String? = null
        var done = false
        FilePicker.show(activity, emptyList(), false) { answer = it; done = true }
        val started = shadowOf(activity).nextStartedActivityForResult
        Extensions.activityResult(started.requestCode, Activity.RESULT_OK, Intent().setData(uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION))
        await { done }
        assertTrue(answer, answer!!.startsWith("["))
        val file = File(JSONArray(answer).getJSONObject(0).getString("path"))
        try {
            assertEquals("bytes", file.readText())
            assertEquals(before, resolver.persistedUriPermissions.map { Triple(it.uri, it.isReadPermission, it.isWritePermission) })
            assertEquals(0, shadow.attempts)
        } finally { file.parentFile!!.deleteRecursively() }
    }

    @Test fun unreadableResultsRemoveEarlierCopies() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val resolver = activity.contentResolver
        val first = Uri.parse("content://picker/one")
        shadowOf(resolver).registerInputStream(first, "first".byteInputStream())
        val cache = java.nio.file.Files.createTempDirectory("exact-picker-test").toFile()
        val second = Uri.parse("content://picker/broken")
        var partialRead = false
        shadowOf(resolver).registerInputStream(second, object : java.io.InputStream() {
            override fun read(): Int = error("bulk read expected")
            override fun read(bytes: ByteArray, offset: Int, length: Int): Int {
                val files = cache.walkTopDown().filter { it.isFile }.toList()
                assertTrue("the first copy must finish before the second fails", files.any { it.readText() == "first" })
                if (!partialRead) {
                    partialRead = true
                    bytes[offset] = 42
                    return 1
                }
                assertTrue("the partial second file must exist", files.any { it.readBytes().contentEquals(byteArrayOf(42)) })
                throw java.io.IOException("provider read failed")
            }
        })
        try {
            val answer = FilePicker.copy(resolver, cache, listOf(first, second))
            assertTrue("second stream was not read", partialRead)
            assertTrue(answer.startsWith("file picker:"))
            assertTrue(cache.listFiles()!!.isEmpty())
        } finally { cache.deleteRecursively() }
    }

    class GrantProvider : android.content.ContentProvider() {
        companion object {
            lateinit var source: File
            lateinit var entered: CountDownLatch
            lateinit var release: CountDownLatch
            val opened = ArrayList<Thread>()
            // Robolectric does not implement URI grant revocation; the provider models read access explicitly.
            var readGrants = emptySet<Uri>()
        }
        override fun onCreate() = true
        override fun getType(uri: Uri) = "image/png"
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?): android.database.Cursor {
            check(Thread.currentThread() != Looper.getMainLooper().thread) { "metadata blocked the UI thread" }
            return android.database.MatrixCursor(arrayOf(android.provider.OpenableColumns.DISPLAY_NAME)).apply { addRow(arrayOf("photo.png")) }
        }
        override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
            check(Thread.currentThread() != Looper.getMainLooper().thread) { "provider open blocked the UI thread" }
            entered.countDown()
            check(release.await(10, TimeUnit.SECONDS)) { "provider was not released" }
            if (uri !in readGrants) throw SecurityException("URI read grant ended")
            opened.add(Thread.currentThread())
            return ParcelFileDescriptor.open(source, ParcelFileDescriptor.MODE_READ_ONLY)
        }
        override fun insert(uri: Uri, values: android.content.ContentValues?) = null
        override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
        override fun update(uri: Uri, values: android.content.ContentValues?, selection: String?, args: Array<out String>?) = 0
    }

    @Test fun workerOpensAfterActivityRecreationUnderAModeledReadGrant() {
        for (revoke in listOf(false, true)) {
            val controller = Robolectric.buildActivity(Activity::class.java).setup()
            val activity = controller.get()
            Extensions.front = { controller.get() }
            Extensions.resumed()
            GrantProvider.source = File.createTempFile("picker-grant", ".png").apply { writeText("image bytes") }
            GrantProvider.entered = CountDownLatch(1); GrantProvider.release = CountDownLatch(1); GrantProvider.opened.clear()
            org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("grants", GrantProvider().apply {
                attachInfo(activity, android.content.pm.ProviderInfo().apply { authority = "grants" })
            })
            val uris = listOf(Uri.parse("content://grants/one"), Uri.parse("content://grants/two"))
            val answers = ArrayList<String?>()
            val done = CountDownLatch(1)
            try {
                FilePicker.show(activity, listOf("image/*"), true) { answers.add(it); done.countDown() }
                val started = shadowOf(activity).nextStartedActivityForResult
                val result = Intent().addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).apply {
                    clipData = ClipData.newRawUri("photos", uris[0]).apply { addItem(ClipData.Item(uris[1])) }
                }
                GrantProvider.readGrants = uris.toSet()
                Extensions.activityResult(started.requestCode, Activity.RESULT_OK, result)
                assertTrue("provider was not opened on a worker", GrantProvider.entered.await(2, TimeUnit.SECONDS))
                assertEquals("callback returned before the provider finished", 1L, done.count)
                controller.recreate()
                assertTrue(activity.isDestroyed)
                assertFalse(activity.isFinishing)
                assertNotSame(activity, controller.get())
                // A negative control proves openFile refuses an absent grant, independently of activity lifetime.
                if (revoke) GrantProvider.readGrants = emptySet()
                GrantProvider.release.countDown()
                await { done.count == 0L }
                assertTrue(activity.contentResolver.persistedUriPermissions.isEmpty())
                if (revoke) {
                    assertEquals(listOf("file picker: URI read grant ended"), answers)
                    assertTrue(GrantProvider.opened.isEmpty())
                } else {
                    assertEquals("the worker opens both files while the modeled grant remains valid: $answers", 2, GrantProvider.opened.size)
                    val paths = JSONArray(answers.single())
                    for (i in 0 until paths.length()) {
                        val file = File(paths.getJSONObject(i).getString("path"))
                        assertEquals("image bytes", file.readText())
                        file.parentFile!!.deleteRecursively()
                    }
                }
            } finally {
                GrantProvider.release.countDown(); GrantProvider.source.delete()
                controller.pause().stop().destroy()
            }
        }
    }

    @Test @GraphicsMode(GraphicsMode.Mode.NATIVE) fun aJpegLabelledHeicExercisesConversionRoutingAndStripsMetadata() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("heic", PhotoProvider())
        val uri = Uri.parse("content://heic/heic-scan")
        // A decodable image with HEIC provider metadata exercises routing and the output codec on the JVM.
        val source = File("../../../scripts/fixtures/picker/oriented-gps.jpg")
        val original = android.media.ExifInterface(source)
        assertNotNull(original.getAttribute(android.media.ExifInterface.TAG_GPS_LATITUDE))
        shadowOf(activity.contentResolver).registerInputStream(uri, source.inputStream())
        var answer: String? = null
        val done = CountDownLatch(1)
        FilePicker.show(activity, listOf(".jpg"), false) { answer = it; done.countDown() }
        val started = shadowOf(activity).nextStartedActivityForResult
        assertEquals(Intent.ACTION_OPEN_DOCUMENT, started.intent.action)
        assertArrayEquals(arrayOf("image/jpeg", "image/heic", "image/heif"), started.intent.getStringArrayExtra(Intent.EXTRA_MIME_TYPES))
        Extensions.activityResult(started.requestCode, Activity.RESULT_OK, Intent().setData(uri))
        await { done.count == 0L }
        assertTrue(answer, answer!!.startsWith("["))
        val picked = JSONArray(answer).getJSONObject(0)
        val file = File(picked.getString("path"))
        try {
            assertEquals("image/jpeg", picked.getString("type"))
            assertEquals("scan.v2", picked.getString("name"))
            assertTrue(picked.getBoolean("converted"))
            assertEquals("jpg", file.extension)
            val exif = android.media.ExifInterface(file)
            assertNull(exif.getAttribute(android.media.ExifInterface.TAG_GPS_LATITUDE))
            assertNull(exif.getAttribute(android.media.ExifInterface.TAG_GPS_LONGITUDE))
            assertTrue(exif.getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, 0) in 0..1)
            val image = javax.imageio.ImageIO.read(file)
            assertEquals(48, image.width)
            assertEquals(64, image.height)
        } finally { file.delete(); file.parentFile!!.delete() }
    }
    @Test fun aKnownExtensionCarriesItsChooserMimeToValidation() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("docx", PhotoProvider())
        val uri = Uri.parse("content://docx/doc")
        shadowOf(activity.contentResolver).registerInputStream(uri, "document".byteInputStream())
        var answer: String? = null
        val done = CountDownLatch(1)
        FilePicker.show(activity, listOf(".docx"), false) { answer = it; done.countDown() }
        val started = shadowOf(activity).nextStartedActivityForResult
        val mime = activity.contentResolver.getType(uri)
        assertEquals(mime, started.intent.type)
        Extensions.activityResult(started.requestCode, Activity.RESULT_OK, Intent().setData(uri))
        await { done.count == 0L }
        val picked = JSONArray(answer).getJSONObject(0)
        val file = File(picked.getString("path"))
        try {
            assertEquals("report", picked.getString("name"))
            assertEquals(mime, picked.getJSONObject("extensions").getString(".docx"))
            assertEquals("docx", picked.getString("extension"))
        } finally { file.delete(); file.parentFile!!.delete() }
    }

    @Test fun aFailedImportLeavesExistingGrantsUntouched() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        Exact.reader = WeakReference(activity)
        Extensions.resumed()
        val resolver = activity.contentResolver
        for (alreadyHeld in listOf(false, true)) {
            val uri = Uri.parse("content://picker/failure-$alreadyHeld")
            if (alreadyHeld) resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            shadowOf(resolver).registerInputStream(uri, object : java.io.InputStream() {
                override fun read(): Int = throw java.io.IOException("read refused")
            })
            var answer: String? = null
            val done = CountDownLatch(1)
            FilePicker.show(activity, emptyList(), false) { answer = it; done.countDown() }
            val started = shadowOf(activity).nextStartedActivityForResult
            Extensions.activityResult(started.requestCode, Activity.RESULT_OK, Intent().setData(uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION))
            await { done.count == 0L }
            assertTrue(answer, answer!!.contains("read refused"))
            assertEquals(alreadyHeld, resolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission })
            if (alreadyHeld) resolver.releasePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
    }

    @Test fun displaySuffixMimeTravelsEvenWhenRustDoesNotKnowTheSuffix() {
        val activity = Robolectric.buildActivity(ExactActivity::class.java).get()
        val resolver = activity.contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("suffix", PhotoProvider())
        val uri = Uri.parse("content://suffix/jpe")
        shadowOf(resolver).registerInputStream(uri, "jpeg".byteInputStream())
        val cache = java.nio.file.Files.createTempDirectory("exact-picker-suffix").toFile()
        try {
            val item = JSONArray(FilePicker.copy(resolver, cache, listOf(uri))).getJSONObject(0)
            assertEquals("photo.jpe", item.getString("name"))
            assertEquals("image/jpeg", item.getString("name_type"))
        } finally { cache.deleteRecursively() }
    }

    @Test fun cacheEvictionBetweenImportsDoesNotRequireAProcessRestart() {
        val cache = java.nio.file.Files.createTempDirectory("picker-eviction").toFile()
        try {
            val imports = PickerImports(cache)
            val first = File(imports.stage(), "source").apply { writeText("first") }
            assertTrue(first.parentFile!!.parentFile!!.deleteRecursively())
            val second = File(imports.stage(), "source").apply { writeText("second") }
            val third = File(imports.stage(), "source").apply { writeText("third") }
            assertEquals("second", second.readText())
            assertEquals("third", third.readText())
            assertNotEquals(second.parentFile, third.parentFile)
        } finally { cache.deleteRecursively() }
    }

    @Test fun aRestartCleansOnlyPickerStaging() {
        val cache = java.nio.file.Files.createTempDirectory("picker-restart").toFile()
        try {
            val before = PickerImports(cache)
            val partial = File(before.stage(), "partial.jpg").apply { writeText("partial") }
            val complete = File(before.stage(), "complete.jpg").apply { writeText("complete") }
            val unrelated = File(cache, "keep").apply { writeText("unrelated") }
            val after = PickerImports(cache)
            assertFalse(partial.parentFile!!.exists())
            assertFalse(complete.parentFile!!.exists())
            assertEquals("unrelated", unrelated.readText())
            val first = after.stage()
            val second = after.stage()
            assertNotEquals(first, second)
            assertTrue(first.isDirectory && second.isDirectory)
            PickerImports(cache)
            assertFalse(first.exists())
            assertFalse(second.exists())
            assertEquals("unrelated", unrelated.readText())
        } finally { cache.deleteRecursively() }
    }
}
