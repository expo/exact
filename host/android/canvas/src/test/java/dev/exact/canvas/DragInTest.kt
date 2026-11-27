package dev.exact.canvas

import android.app.Activity
import android.content.ClipData
import android.content.ClipDescription
import android.net.Uri
import android.provider.DocumentsContract
import android.view.DragEvent
import android.view.View
import android.widget.EditText
import android.widget.FrameLayout
import org.junit.Assert.*
import org.json.JSONObject
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.File

/** Drags from other apps (LLP 1094.001 D5): what a drop's items are, which drags the window takes, which a field gives
 * away, and the copies a drop's files become. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29, 33], manifest = Config.NONE, shadows = [DragInTest.Journal::class])
class DragInTest {
    @Before fun begin() { Journal.lines.clear(); Journal.drags.clear(); Journal.release = 0; Journal.owner = 2; Journal.asked = 0; Journal.lastFieldDrop = null }

    /** exact2's journal, as the reader writes to it with no owner. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Journal {
        companion object {
            val lines = ArrayList<String>()
            /** What the reader told exact2 of drags, in order. */
            val drags = ArrayList<String>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun journal(line: String) { lines += line }
            @JvmStatic @Implementation fun dragOver(x: Float, y: Float) { drags += "over ${x.toInt()} ${y.toInt()}" }
            @JvmStatic @Implementation fun dragLeft() { drags += "left" }
            @JvmStatic @Implementation fun dragDropped(x: Float, y: Float) { drags += "dropped ${x.toInt()} ${y.toInt()}" }
            /** The drop number exact2 answers a release with (0: nothing takes it). */
            var release = 0
            /** Who takes a drop on a field's view, as exact2 answers as it lands: -1 not the field, 0 the field, 1 its
             * own node, 2 one above it. */
            var owner = 2
            @JvmStatic @Implementation fun dropReleased(x: Float, y: Float): Int { drags += "released ${x.toInt()} ${y.toInt()}"; return release }
            @JvmStatic @Implementation fun dropLanded(id: Int, items: String) { drags += "landed $id $items" }
            /** How often exact2 was asked who takes a drop on a field. */
            var asked = 0
            var lastFieldDrop: Pair<Int, Int>? = null
            @JvmStatic @Implementation fun fieldDrop(id: Int, x: Float, y: Float): Int { asked++; lastFieldDrop = x.toInt() to y.toInt(); return owner }
        }
    }

    private fun clip(vararg items: ClipData.Item): ClipData =
        ClipData(ClipDescription("drag", arrayOf(ClipDescription.MIMETYPE_TEXT_PLAIN)), items.first()).apply { items.drop(1).forEach(::addItem) }

    @Test fun textItemsJoinAndTheFirstWebLinkAmongThemIsTheLink() {
        val items = DropItems.read(clip(ClipData.Item("Mount Fuji"), ClipData.Item("https://example.com/fuji"),
            ClipData.Item("https://example.com/later"), ClipData.Item("Japan")))
        assertEquals(DropItems(emptyList(), "Mount Fuji\nhttps://example.com/later\nJapan", "https://example.com/fuji"), items)
        assertEquals("a link inside text stays text", "",
            DropItems.read(clip(ClipData.Item("see https://example.com/fuji"))).url)
        assertEquals("Chrome's link: its title, a line break, its address", DropItems(emptyList(), "A link to Fuji", "https://example.com/fuji"),
            DropItems.read(clip(ClipData.Item("A link to Fuji\nhttps://example.com/fuji"))))
    }

    @Test fun aContentUriIsAFileAFileUriIsNoneAndAnyOtherIsTheLink() {
        val photo = Uri.parse("content://photos/1")
        val note = Uri.parse("file:///sdcard/note.txt")
        val items = DropItems.read(clip(ClipData.Item(photo), ClipData.Item(Uri.parse("mailto:a@example.com")), ClipData.Item(note),
            ClipData.Item(Uri.parse("https://example.com/second"))))
        assertEquals("a file: URI is read with the app's own rights, so it is no file to take",
            DropItems(listOf(photo), "", "mailto:a@example.com"), items)
        assertEquals("a raw web link", "https://example.com/",
            DropItems.read(ClipData.newRawUri("link", Uri.parse("https://example.com/"))).url)
        assertEquals("the reader's JSON", """{"paths":["\/a"],"text":"","url":"mailto:a@example.com"}""", items.json(listOf("/a")))
    }

    @Test fun aDragOfOnlyAnAppOrAnIntentFindsNoTarget() {
        fun carries(vararg types: String) = DragIn.carries(ClipDescription("drag", types))
        assertFalse(carries("application/vnd.android.activity"))
        assertFalse(carries(ClipDescription.MIMETYPE_TEXT_INTENT, "application/vnd.android.task"))
        assertTrue(carries(ClipDescription.MIMETYPE_TEXT_PLAIN))
        assertTrue(carries(ClipDescription.MIMETYPE_TEXT_URILIST))
        assertTrue(carries("application/vnd.android.shortcut", "image/png"))
        assertFalse("an app's own type", carries("chrome/tab"))
        assertTrue("an email is a file", carries("message/rfc822"))
        assertFalse(DragIn.carries(null))
    }

    @Test fun eachFileIsCopiedUnderItsOwnNameAndAFolderOrAnUnreadableItemIsLeftOut() {
        val context = RuntimeEnvironment.getApplication()
        val resolver = context.contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("names", FilePickerTest.PhotoProvider())
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("folders", Folders())
        val (first, second, folder, unread) = listOf("content://names/a", "content://names/b", "content://folders/f", "content://names/c").map(Uri::parse)
        shadowOf(resolver).registerInputStream(first, "first".byteInputStream())
        shadowOf(resolver).registerInputStream(second, "second".byteInputStream())
        val into = DropStaging.next(context)
        val paths = DropItems.copy(resolver, into, listOf(first, folder, second, unread), Journal.lines::add)
        assertEquals(listOf("photo.png", "photo.png"), paths.map { File(it).name })
        assertEquals(listOf("first", "second"), paths.map { File(it).readText() })
        assertTrue(paths.all { File(it).canonicalPath.startsWith(into.canonicalPath) })
        assertEquals(listOf("drop: a folder was left out: a drop takes files"), Journal.lines.take(1))
        assertTrue(Journal.lines[1], Journal.lines.size == 2 && Journal.lines[1].startsWith("drop: an item could not be read"))
        assertEquals("the unread item leaves no folder", listOf("0", "2"), into.list()!!.sorted())
        assertNotEquals("each drop has a folder of its own", into, DropStaging.next(context))
    }

    @Test fun aTextFileStaysTheFieldsAndAnyOtherContentItemIsAFile() {
        val resolver = RuntimeEnvironment.getApplication().contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("typed", Typed())
        fun holds(vararg uris: String) = DragIn.holdsFile(ClipData("drag", arrayOf(ClipDescription.MIMETYPE_TEXT_URILIST), ClipData.Item(Uri.parse(uris[0])))
            .apply { uris.drop(1).forEach { addItem(ClipData.Item(Uri.parse(it))) } }, resolver)
        assertFalse(holds("content://typed/notes.txt"))
        assertTrue(holds("content://typed/notes.txt", "content://typed/photo.png"))
        assertTrue("a type the provider does not say", holds("content://typed/unknown"))
        assertFalse("a web link", holds("https://example.com/"))
    }

    class Typed : android.content.ContentProvider() {
        override fun onCreate() = true
        override fun getType(uri: Uri) = when (uri.lastPathSegment) { "notes.txt" -> "text/plain"; "photo.png" -> "image/png"; else -> null }
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?) = null
        override fun insert(uri: Uri, values: android.content.ContentValues?) = null
        override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
        override fun update(uri: Uri, values: android.content.ContentValues?, selection: String?, args: Array<out String>?) = 0
    }

    /** A drag event as Android's window sends one: the platform's own `DragEvent.obtain`, whose parameters differ by
     * API level, given each by its type (the action, the point and then zero offsets, the description and the clip). */
    private fun event(action: Int, x: Float = 0f, y: Float = 0f, clip: ClipData? = null, description: ClipDescription? = clip?.description, local: Any? = null): DragEvent {
        val obtain = DragEvent::class.java.declaredMethods
            .filter { it.name == "obtain" && it.parameterTypes.firstOrNull() == Int::class.javaPrimitiveType }
            .maxBy { it.parameterCount }
        val floats = ArrayDeque(listOf(x, y))
        val args = obtain.parameterTypes.map { type ->
            when (type) {
                Int::class.javaPrimitiveType -> action
                Float::class.javaPrimitiveType -> floats.removeFirstOrNull() ?: 0f
                Boolean::class.javaPrimitiveType -> false
                ClipDescription::class.java -> description
                ClipData::class.java -> clip
                Any::class.java -> local
                else -> null
            }
        }
        return obtain.invoke(null, *args.toTypedArray()) as DragEvent
    }

    /** Two fields, ids 1 and 2, 300 by 60 at (20, 40) and (20, 200), on a root as the reader lays them out, the second
     * with [state] (1 placed, 4 its node takes drops). The root, the fields and their texts. */
    private class Two(val root: Root, val fields: Fields, val first: FieldText, val second: FieldText) {
        fun tell(state: Int, clipSecond: Boolean = false) = fields.told(FieldsTest.Companion.statement(listOf(
            FieldsTest.Companion.Told(1, 20f, 40f, 300f, 60f, 1),
            FieldsTest.Companion.Told(2, 20f, 200f, 300f, 60f, state, clip = if (clipSecond) floatArrayOf(0f, 0f, 400f, 100f) else null),
        )))
    }

    private fun twoFields(state: Int, clipSecond: Boolean = false): Two {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val root = Root(activity)
        val fields = Fields(activity)
        root.addView(fields, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        activity.setContentView(root)
        val two = Two(root, fields, FieldText(activity), FieldText(activity))
        two.tell(state, clipSecond)
        root.measure(View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY))
        root.layout(0, 0, 400, 400)
        val texts = (0 until fields.childCount).map { (fields.getChildAt(it) as FrameLayout).getChildAt(0) as FieldText }
        return Two(root, fields, texts.first { it.text.toString() == "1" }, texts.first { it.text.toString() == "2" })
    }

    /** A drag of [clip] started, over ([x], [y]), then, once [hovered] ran, dropped there. */
    private fun drag(root: Root, x: Float, y: Float, clip: ClipData = ClipData.newPlainText("drag", "Fuji"), local: Any? = null, hovered: () -> Unit = {}): Boolean {
        root.dispatchDragEvent(event(DragEvent.ACTION_DRAG_STARTED, description = clip.description, local = local))
        root.dispatchDragEvent(event(DragEvent.ACTION_DRAG_LOCATION, x, y, description = clip.description, local = local))
        hovered()
        val took = root.dispatchDragEvent(event(DragEvent.ACTION_DROP, x, y, clip, local = local))
        root.dispatchDragEvent(event(DragEvent.ACTION_DRAG_ENDED, description = clip.description, local = local))
        return took
    }

    @Test fun aTextDropReachesEitherOfTwoFieldsWhichInsertsIt() {
        val two = twoFields(state = 1)
        drag(two.root, 170f, 230f)
        assertTrue(two.second.text.toString(), two.second.text.toString().contains("Fuji"))
        drag(two.root, 170f, 70f)
        assertTrue(two.first.text.toString(), two.first.text.toString().contains("Fuji"))
        assertEquals("each field told where the drag was, then where it took the drop",
            listOf("over 170 230", "dropped 170 230", "over 170 70", "dropped 170 70"), Journal.drags)
    }

    @Test fun aFieldWhoseNodeTakesDropsGivesTheDragToTheRoot() {
        val two = twoFields(state = 1 or 4)
        Journal.owner = 1
        drag(two.root, 170f, 230f)
        assertEquals("2", two.second.text.toString())
        assertEquals(listOf("over 170 230", "released 170 230"), Journal.drags)
    }

    @Test fun aFieldDecidesAtTheDropByWhatExact2AnswersThenNotByTheFramesState() {
        val two = twoFields(state = 1)
        Journal.owner = 2
        drag(two.root, 170f, 230f) { assertEquals("the hover asks exact2 nothing", 0, Journal.asked); Journal.owner = 1 }
        assertEquals("exact2 is asked as the drop lands", 1, Journal.asked)
        assertEquals("its node takes drops as the drop lands", "2", two.second.text.toString())
        assertEquals(listOf("over 170 230", "released 170 230"), Journal.drags)
        Journal.drags.clear()
        val owned = twoFields(state = 1 or 4)
        Journal.owner = 1
        drag(owned.root, 170f, 230f) { assertEquals("the hover asks exact2 nothing", 1, Journal.asked); Journal.owner = 2 }
        assertEquals("exact2 is asked as the drop lands", 2, Journal.asked)
        assertTrue("its node took drops at the start and no longer does", owned.second.text.toString().contains("Fuji"))
        assertEquals(listOf("over 170 230", "dropped 170 230"), Journal.drags)
    }

    @Test fun aNodePaintedOverTheFieldTakesTheDropThere() {
        val two = twoFields(state = 1)
        Journal.owner = -1
        drag(two.root, 170f, 230f)
        assertEquals("2", two.second.text.toString())
        assertEquals(listOf("over 170 230", "released 170 230"), Journal.drags)
    }

    @Test fun aRawContentUriThatIsAFileGoesAboveTheFieldAndARawWebLinkStays() {
        val resolver = RuntimeEnvironment.getApplication().contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("typed", Typed())
        val two = twoFields(state = 1)
        drag(two.root, 170f, 230f, ClipData.newRawUri("photo", Uri.parse("content://typed/photo.png")))
        assertEquals("2", two.second.text.toString())
        drag(two.root, 170f, 230f, ClipData.newRawUri("link", Uri.parse("https://example.com/fuji")))
        assertTrue(two.second.text.toString(), two.second.text.toString().contains("https://example.com/fuji"))
        assertEquals(listOf("over 170 230", "released 170 230", "over 170 230", "dropped 170 230"), Journal.drags)
        assertTrue(resolver.getType(Uri.parse("content://typed/photo.png")) == "image/png")
    }

    /** An item's bytes that the test hands out: the read waits, once it has begun, until [release] or for at most
     * 10 seconds, and fails at once on the thread that made it, which a copy on the main thread would block. */
    private class Held : java.io.InputStream() {
        private val maker = Thread.currentThread()
        val reading = java.util.concurrent.CountDownLatch(1)
        private val released = java.util.concurrent.CountDownLatch(1)
        @Volatile var reader: Thread? = null
        private val bytes = "pixels".byteInputStream()
        fun release() = released.countDown()
        override fun read(): Int = held { bytes.read() }
        override fun read(b: ByteArray, off: Int, len: Int): Int = held { bytes.read(b, off, len) }
        private fun held(read: () -> Int): Int {
            val thread = Thread.currentThread()
            if (thread === maker) throw AssertionError("the copy read on the main thread")
            reader = thread
            reading.countDown()
            if (!released.await(10, java.util.concurrent.TimeUnit.SECONDS)) throw AssertionError("the drop waited for its copy")
            return read()
        }
    }

    @Test fun aDropTheWindowTakesLandsOnceWhenItsCopiesEnd() {
        val resolver = RuntimeEnvironment.getApplication().contentResolver
        org.robolectric.shadows.ShadowContentResolver.registerProviderInternal("names", FilePickerTest.PhotoProvider())
        val photo = Uri.parse("content://names/a")
        val held = Held()
        shadowOf(resolver).registerInputStream(photo, held)
        val landed = { Journal.drags.filter { it.startsWith("landed") } }
        fun idle() = shadowOf(android.os.Looper.getMainLooper()).idle()
        try {
            val two = twoFields(state = 1 or 4)
            Journal.release = 7
            val clip = ClipData("photo", arrayOf("image/png"), ClipData.Item(photo)).apply { addItem(ClipData.Item("Glasshouse")) }
            assertTrue(drag(two.root, 200f, 330f, clip, local = DragOut.OWN))
            assertTrue("the copy began reading", held.reading.await(5, java.util.concurrent.TimeUnit.SECONDS))
            idle()
            assertEquals("the drag ended, and its copy is still reading", emptyList<String>(), landed())
        } finally {
            held.release()
            held.reader?.join(5000)
        }
        assertFalse("the copy ended", held.reader!!.isAlive)
        idle()
        assertEquals("one landing, once the copy ended", 1, landed().size)
        val items = JSONObject(landed().single().removePrefix("landed 7 "))
        assertEquals("Glasshouse", items.getString("text"))
        assertEquals("pixels", File(items.getJSONArray("paths").getString(0)).readText())
    }

    @Test fun anotherAppsDropWithNoReadPermissionDeliversItsTextAlone() {
        val two = twoFields(state = 1 or 4)
        Journal.release = 7
        val clip = ClipData("photo", arrayOf("image/png"), ClipData.Item(Uri.parse("content://names/a"))).apply { addItem(ClipData.Item("Glasshouse")) }
        assertTrue(drag(two.root, 200f, 330f, clip))
        shadowOf(android.os.Looper.getMainLooper()).idle()
        assertEquals("landed 7 " + DropItems(emptyList(), "Glasshouse", "").json(emptyList()), Journal.drags.last())
        assertEquals(listOf("drop: Android granted no read permission for its files, so it delivers its text and link alone"), Journal.lines)
    }

    @Test fun aFieldClippedAwayWholeHasAPortNoDragFinds() {
        val two = twoFields(state = 1, clipSecond = true)
        assertEquals(listOf(0, 0), (two.second.parent as View).let { listOf(it.width, it.height) })
        drag(two.root, 170f, 230f)
        assertEquals("2", two.second.text.toString())
    }

    @Test fun insetPageTranslatesRootAndFieldDragsIntoPagePixels() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val root = Root(activity)
        val fields = Fields(activity)
        val page = View(activity)
        val insetX = 48
        val insetY = 96
        fun insetParams() = FrameLayout.LayoutParams(400 - insetX, 400 - insetY).apply {
            gravity = android.view.Gravity.TOP or android.view.Gravity.LEFT
            leftMargin = insetX
            topMargin = insetY
        }
        root.addView(fields, insetParams())
        root.addView(page, insetParams())
        root.drags.page = page
        activity.setContentView(root)
        val two = Two(root, fields, FieldText(activity), FieldText(activity))
        two.tell(state = 1)
        root.measure(View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY))
        root.layout(0, 0, 400, 400)
        val texts = (0 until fields.childCount).map { (fields.getChildAt(it) as FrameLayout).getChildAt(0) as FieldText }
        val second = texts.first { it.text.toString() == "2" }

        // A drag over the page outside any field goes through Root.onDragEvent and subtracts the page inset.
        drag(root, insetX + 10f, insetY + 15f)
        assertEquals(listOf("over 10 15", "released 10 15"), Journal.drags)

        // A drag onto a hosted field kept by the field reports page-local coordinates to dragOver, fieldDrop and dragDropped.
        Journal.drags.clear()
        Journal.owner = 2
        drag(root, insetX + 170f, insetY + 230f)
        assertEquals(170 to 230, Journal.lastFieldDrop)
        assertEquals(listOf("over 170 230", "dropped 170 230"), Journal.drags)
        assertTrue(second.text.toString(), second.text.toString().contains("Fuji"))

        // A drag onto a hosted field taken by an Exact drop node reports page-local coordinates to fieldDrop and dropReleased.
        Journal.drags.clear()
        Journal.owner = 1
        drag(root, insetX + 170f, insetY + 70f)
        assertEquals(170 to 70, Journal.lastFieldDrop)
        assertEquals(listOf("over 170 70", "released 170 70"), Journal.drags)
    }

    class Folders : android.content.ContentProvider() {
        override fun onCreate() = true
        override fun getType(uri: Uri) = DocumentsContract.Document.MIME_TYPE_DIR
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?) = null
        override fun insert(uri: Uri, values: android.content.ContentValues?) = null
        override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
        override fun update(uri: Uri, values: android.content.ContentValues?, selection: String?, args: Array<out String>?) = 0
    }
}
