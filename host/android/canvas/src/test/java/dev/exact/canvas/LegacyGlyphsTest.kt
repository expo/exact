package dev.exact.canvas

import org.junit.Assert.*
import org.junit.Test

/** What bounds the legacy glyph cache (`LegacyGlyphs`, Android 10 and 11): a face is kept by identity only while
 * something else holds it, and a face's outlines and the images are each the most recent few. Drawing reads outlines
 * through the native font reader, which a JVM test has not, so these test the two maps it keeps them in. */
class LegacyGlyphsTest {
    @Test fun aFaceIsFoundByIdentity() {
        val faces = WeakIdentityMap<String, Int>()
        val a = String(charArrayOf('a'))
        val b = String(charArrayOf('a'))
        assertEquals(1, faces.getOrPut(a) { 1 })
        assertEquals(2, faces.getOrPut(b) { 2 })
        assertEquals(1, faces.getOrPut(a) { 3 })
        assertEquals(2, faces.size)
    }

    /** A reader made again makes its faces again: the faces the old one let go leave the map. */
    @Test fun aFaceLetGoIsCollected() {
        val faces = WeakIdentityMap<Any, Recent<Int, Any>>()
        val kept = Any()
        faces.getOrPut(kept) { Recent(4) }
        repeat(1000) { faces.getOrPut(Any()) { Recent(4) } }
        var tries = 0
        while (faces.size > 1 && tries++ < 100) {
            System.gc()
            Thread.sleep(10)
        }
        assertEquals("only the face still held is kept", 1, faces.size)
        assertNotNull(faces.getOrPut(kept) { error("the held face was dropped") })
    }

    @Test fun recentKeepsTheMostRecentlyUsed() {
        val outlines = Recent<Int, String>(3)
        for (id in 1..3) outlines[id] = "$id"
        outlines[1]
        outlines[4] = "4"
        assertEquals(setOf(1, 3, 4), outlines.keys)
    }
}
