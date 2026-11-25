package exact.clipboard

import android.graphics.BitmapFactory
import android.graphics.Color
import android.os.CancellationSignal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.io.RandomAccessFile
import java.util.Base64

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AndroidImageTest {
    @get:Rule val folder = TemporaryFolder()
    private val source get() = File(folder.root, "source")
    private val directory get() = File(folder.root, "tmp")

    private fun sanitize(remaining: Int = MAX_IMAGE_BYTES) =
        sanitizedPNG(source, directory, remaining, CancellationSignal(), Long.MAX_VALUE)

    @Test fun checksCompressedBytesAndHeaderDimensionsBeforeAllocatingPixels() {
        assertThrows(InvalidImage::class.java) { checkPixels(0, 8000) }
        RandomAccessFile(source, "rw").use { it.setLength(MAX_IMAGE_BYTES + 1L) }
        assertThrows(OverLimit::class.java) { sanitize() }
        // The 9000 × 9000 header from the clipboard fixture crosses the pixel limit on its own.
        source.writeBytes(Base64.getDecoder().decode("iVBORw0KGgoAAAANSUhEUgAAIygAACMoCAIAAADit+XtAAAAC0lEQVR4nGNggAEAAAoAAX+AdF4AAAAASUVORK5CYII="))
        assertThrows(OverLimit::class.java) { sanitize() }
        source.writeText("ordinary text advertised as image/png")
        assertThrows(InvalidImage::class.java) { sanitize() }
        assertFalse(directory.exists())
    }

    @Test fun stripsTextChunksAndKeepsOnlyCompletePNGsWithinTheSharedBudget() {
        source.writeBytes(Base64.getDecoder().decode("iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAKHRFWHRDb21tZW50AG1ldGFkYXRhIGEgc2FuaXRpemVkIHdyaXRlIGRyb3BzdXr2SAAAABFJREFUeNpj+M/AgBUxDC0JACj/P8HOd8hPAAAAAElFTkSuQmCC"))
        val file = sanitize()
        val bitmap = BitmapFactory.decodeFile(file.path)
        assertEquals(8, bitmap.width)
        assertEquals(Color.RED, bitmap.getPixel(0, 0))
        assertFalse(file.readBytes().toString(Charsets.ISO_8859_1).contains("tEXt"))
        assertThrows(OverLimit::class.java) { sanitize(file.length().toInt() - 1) }
        assertEquals(listOf(file), directory.listFiles()!!.toList())
    }

    @Test fun anAnimationUsesItsFirstFrameEvenWithASeparateDefaultImage() {
        source.writeBytes(Base64.getDecoder().decode("iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAACGFjVEwAAAACAAAAAPONk3AAAAARSURBVHjaY/jPwIAVMQwtCQAo/z/BznfITwAAABpmY1RMAAAAAAAAAAgAAAAIAAAAAAAAAAAAAQAKAADyTrpiAAAAFGZkQVQAAAABeNpjYPjPgB0NLQkA6TA/wSwJCQUAAAAaZmNUTAAAAAIAAAAIAAAACAAAAAAAAAAAAAEACgAAH9hpiwAAABRmZEFUAAAAA3jaY2Bg+I8DDSkJAKlwP8EZfwA/AAAAAElFTkSuQmCC"))
        assertEquals(Color.GREEN, BitmapFactory.decodeFile(sanitize().path).getPixel(0, 0))
        val bytes = source.readBytes()
        val control = bytes.toString(Charsets.ISO_8859_1).indexOf("fcTL")
        bytes[control + 4 + 26] = (bytes[control + 4 + 26].toInt() xor 1).toByte()
        source.writeBytes(bytes)
        assertThrows(InvalidImage::class.java) { sanitize() }
    }
}
