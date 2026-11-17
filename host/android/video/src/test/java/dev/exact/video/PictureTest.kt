package dev.exact.video

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The picture's rect and the rendition's viewport (LLP 1042.001 D5, D6). */
class PictureTest {
    private fun near(want: Rect, got: Rect) {
        assertEquals(want.left, got.left, 0.01f); assertEquals(want.top, got.top, 0.01f)
        assertEquals(want.width, got.width, 0.01f); assertEquals(want.height, got.height, 0.01f)
    }

    @Test fun containLetterboxes() = near(Rect(0f, 224.75f, 1000f, 562.5f), Picture.rect("contain", 1000, 1012, 1920, 1080))

    @Test fun coverFillsAndCrops() = near(Rect(-399.56f, 0f, 1799.11f, 1012f), Picture.rect("cover", 1000, 1012, 1920, 1080))

    @Test fun noneIsTheVideosSizeCentred() = near(Rect(-460f, -34f, 1920f, 1080f), Picture.rect("none", 1000, 1012, 1920, 1080))

    @Test fun scaleDownNeverEnlarges() = near(Rect(340f, 386f, 320f, 240f), Picture.rect("scale-down", 1000, 1012, 320, 240))

    @Test fun anUnknownSizeIsTheBox() =
        assertEquals(Rect(0f, 0f, 1000f, 1012f), Picture.rect("contain", 1000, 1012, 0, 0))

    @Test fun theViewportIsTheBoxButForCoverAndNone() {
        assertEquals(974 to 1090, Picture.viewport("contain", 974, 1090, 1920, 1080))
        assertEquals(974 to 1090, Picture.viewport("scale-down", 974, 1090, 1920, 1080))
        assertEquals(974 to 1090, Picture.viewport("fill", 974, 1090, 1920, 1080))
        assertEquals(1938 to 1090, Picture.viewport("cover", 974, 1090, 1920, 1080))
        assertEquals(974 to 1090, Picture.viewport("cover", 974, 1090, 0, 0))
        assertNull(Picture.viewport("none", 974, 1090, 1920, 1080))
        assertNull(Picture.viewport("contain", 0, 0, 1920, 1080))
    }

    @Test fun resolutionReadsWidthByHeight() {
        assertEquals(960 to 540, Picture.resolution("960x540"))
        assertNull(Picture.resolution("960"))
        assertNull(Picture.resolution(null))
        assertNull(Picture.resolution("0x540"))
    }
}
