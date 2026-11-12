package exact.quickactions

import android.content.Context
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.Icon
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers
import org.xmlpull.v1.XmlPullParser

/** Run with -Papp=shelf: the shell generates its shortcut XML and vectors from the real manifest and symbol table. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], shadows = [ShortcutIconsTest.Session::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ShortcutIconsTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Session {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = false
        }
    }
    private val app: Context get() = RuntimeEnvironment.getApplication()
    private val manager get() = app.getSystemService(ShortcutManager::class.java)
    private val module: QuickActionsModule get() {
        val constructor = ExtensionContext::class.java.getDeclaredConstructor(Context::class.java, String::class.java)
        constructor.isAccessible = true
        return QuickActionsModule(constructor.newInstance(app, "quick-actions"))
    }
    private val android = "http://schemas.android.com/apk/res/android"

    private fun item(symbol: String?) = JSONObject().put("name", "Continue Piranesi").put("url", "/book/seed-2")
        .put("apple", JSONObject().also { if (symbol != null) it.put("symbol", symbol) })

    private fun call(module: QuickActionsModule, name: String, args: JSONObject): JSONObject {
        var result: JSONObject? = null
        module.later(name, args) { result = it }
        return requireNotNull(result).also { assertFalse(it.toString(), it.has("error")) }
    }

    private fun dynamic(symbol: String?): ShortcutInfo {
        call(module, "setAppShortcuts", JSONObject().put("shortcuts", JSONArray().put(item(symbol))))
        return manager.dynamicShortcuts.single()
    }

    private fun icon(info: ShortcutInfo): Icon? = ReflectionHelpers.callInstanceMethod(info, "getIcon")

    private fun visibleVector(id: Int) {
        assertTrue("shortcut must name an icon resource", id != 0)
        val drawable = app.getDrawable(id)!!
        val bitmap = Bitmap.createBitmap(48, 48, Bitmap.Config.ARGB_8888)
        drawable.setBounds(0, 0, 48, 48)
        drawable.draw(Canvas(bitmap))
        val pixels = IntArray(48 * 48)
        bitmap.getPixels(pixels, 0, 48, 0, 0, 48, 48)
        assertTrue("the vector draws visible ink", pixels.any { it ushr 24 != 0 })
    }

    @Test fun shelfsStaticShortcutNamesTheSameVectorAsItsDynamicSymbol() {
        val id = app.resources.getIdentifier("exact_shortcuts", "xml", app.packageName)
        var staticIcon = 0
        app.resources.getXml(id).use { xml ->
            while (xml.next() != XmlPullParser.END_DOCUMENT) {
                if (xml.eventType == XmlPullParser.START_TAG && xml.name == "shortcut" &&
                    xml.getAttributeValue(android, "shortcutId") == "manifest-0") {
                    staticIcon = xml.getAttributeResourceValue(android, "icon", 0)
                }
            }
        }
        visibleVector(staticIcon)
        val icon = requireNotNull(icon(dynamic("plus")))
        assertEquals(Icon.TYPE_RESOURCE, icon.type)
        assertEquals(staticIcon, icon.resId)
    }

    @Test fun dynamicAndPinnedShortcutsCarryTheDeclaredBookSymbol() {
        val dynamic = dynamic("book")
        val icon = requireNotNull(icon(dynamic))
        assertEquals(Icon.TYPE_RESOURCE, icon.type)
        visibleVector(icon.resId)
        assertEquals("Continue Piranesi", dynamic.longLabel)
        assertEquals("/book/seed-2", dynamic.intent!!.data.toString())
        shadowOf(manager).setIsRequestPinShortcutSupported(true)
        val reply = call(module, "requestPinShortcut", JSONObject().put("shortcut", item("book")))
        assertEquals("requested", reply.getString("state"))
        val pinned = requireNotNull(icon(manager.pinnedShortcuts.single()))
        assertEquals(Icon.TYPE_RESOURCE, pinned.type)
        assertEquals(icon.resId, pinned.resId)
    }

    @Test fun missingAndUnknownSymbolsLeaveTheIconUnset() {
        for (symbol in listOf(null, "", "no-such-symbol", "sf/flag.checkered", "sf/arrow_back")) {
            assertNull(symbol, icon(dynamic(symbol)))
            shadowOf(manager).setIsRequestPinShortcutSupported(true)
            call(module, "requestPinShortcut", JSONObject().put("shortcut", item(symbol)))
            assertNull(symbol, icon(manager.pinnedShortcuts.single()))
        }
    }

    @Test fun symbolSpellingsUseTheSameMaterialAndPortableVectors() {
        for (names in listOf(
            listOf("add", "plus", "sf/plus"),
            listOf("back", "chevron.backward", "chevron_backward", "arrow_back_ios", "sf/chevron.backward"),
            listOf("bookmark-fill", "bookmark.fill", "bookmark_fill", "sf/bookmark.fill"),
        )) {
            val ids = names.map { requireNotNull(icon(dynamic(it))) { "no icon for $it" }.resId }
            assertEquals(names.toString(), 1, ids.toSet().size)
            visibleVector(ids.first())
        }
        val back = requireNotNull(icon(dynamic("back"))).resId
        app.resources.getXml(back).use { xml ->
            while (xml.next() != XmlPullParser.END_DOCUMENT) {
                if (xml.eventType == XmlPullParser.START_TAG && xml.name == "path") {
                    assertEquals("M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z",
                        xml.getAttributeValue(android, "pathData"))
                    assertEquals(1, xml.getAttributeIntValue(android, "fillType", -1))
                }
            }
        }
    }
}
