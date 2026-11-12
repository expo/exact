package dev.exact.canvas

import android.media.ExifInterface
import java.awt.image.BufferedImage
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import javax.imageio.ImageIO
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class ImageCodecTest {
    private fun jpeg(width: Int = 6, height: Int = 4, progressive: Boolean = false): ByteArray = ByteArrayOutputStream().use {
        val writer = ImageIO.getImageWritersByFormatName("jpeg").next()
        try {
            ImageIO.createImageOutputStream(it).use { output ->
                writer.output = output
                val options = writer.defaultWriteParam
                if (progressive) options.progressiveMode = javax.imageio.ImageWriteParam.MODE_DEFAULT
                writer.write(null, javax.imageio.IIOImage(BufferedImage(width, height, BufferedImage.TYPE_INT_RGB), null, null), options)
            }
        } finally {
            writer.dispose()
        }
        it.toByteArray()
    }

    private fun png(): ByteArray = ByteArrayOutputStream().use {
        ImageIO.write(BufferedImage(3, 2, BufferedImage.TYPE_INT_RGB), "png", it)
        it.toByteArray()
    }

    private fun webp(): ByteArray = java.util.Base64.getDecoder()
        .decode("UklGRiIAAABXRUJQVlA4IBYAAAAwAQCdASoBAAEADsD+JaQAA3AAAAAA")

    private fun intBytes(value: Int, order: java.nio.ByteOrder): ByteArray =
        java.nio.ByteBuffer.allocate(4).order(order).putInt(value).array()

    private fun pngText(image: ByteArray, name: String, text: String): ByteArray {
        val data = text.toByteArray()
        val chunk = name.toByteArray() + data
        val crc = java.util.zip.CRC32().apply { update(chunk) }.value.toInt()
        val end = image.size - 12
        return image.copyOfRange(0, end) + intBytes(data.size, java.nio.ByteOrder.BIG_ENDIAN) +
            chunk + intBytes(crc, java.nio.ByteOrder.BIG_ENDIAN) + image.copyOfRange(end, image.size)
    }

    private fun webpXmp(image: ByteArray): ByteArray {
        val data = xml.toByteArray()
        val chunk = "XMP ".toByteArray() + intBytes(data.size, java.nio.ByteOrder.LITTLE_ENDIAN) +
            data + if (data.size % 2 == 1) byteArrayOf(0) else byteArrayOf()
        val out = "RIFF".toByteArray() + intBytes(image.size - 8 + chunk.size, java.nio.ByteOrder.LITTLE_ENDIAN) +
            image.copyOfRange(8, image.size) + chunk
        out[20] = (out[20].toInt() or 4).toByte() // VP8X's XMP flag.
        return out
    }

    private fun segment(marker: Int, payload: ByteArray): ByteArray {
        val size = payload.size + 2
        return byteArrayOf(-1, marker.toByte(), (size shr 8).toByte(), size.toByte()) + payload
    }

    private fun dataset(number: Int, text: String): ByteArray {
        val bytes = text.toByteArray()
        return byteArrayOf(0x1c, 2, number.toByte(), (bytes.size shr 8).toByte(), bytes.size.toByte()) + bytes
    }

    private fun photoshop(iptc: ByteArray): ByteArray {
        val resource = "8BIM".toByteArray() + byteArrayOf(4, 4, 0, 0) +
            byteArrayOf(0, 0, (iptc.size shr 8).toByte(), iptc.size.toByte()) + iptc +
            if (iptc.size % 2 == 1) byteArrayOf(0) else byteArrayOf()
        return segment(0xed, "Photoshop 3.0\u0000".toByteArray() + resource)
    }

    private fun withMetadata(image: ByteArray, vararg segments: ByteArray): ByteArray =
        image.copyOfRange(0, 2) + segments.fold(byteArrayOf()) { a, b -> a + b } + image.copyOfRange(2, image.size)

    private fun tagged(image: ByteArray, xmp: String? = null): ByteArray {
        val file = File.createTempFile("image-codec-test", ".jpg")
        return try {
            file.writeBytes(image)
            ExifInterface(file.path).apply {
                setAttribute(ExifInterface.TAG_ORIENTATION, "6")
                setAttribute(ExifInterface.TAG_PIXEL_X_DIMENSION, "600")
                setAttribute(ExifInterface.TAG_PIXEL_Y_DIMENSION, "400")
                setAttribute(ExifInterface.TAG_DATETIME_ORIGINAL, "2026:10:24 12:34:56")
                setAttribute(ExifInterface.TAG_OFFSET_TIME, "+01:00")
                setAttribute(ExifInterface.TAG_OFFSET_TIME_ORIGINAL, "+02:00")
                setAttribute(ExifInterface.TAG_OFFSET_TIME_DIGITIZED, "+03:00")
                setAttribute(ExifInterface.TAG_MAKE, "Exact camera")
                setAttribute(ExifInterface.TAG_GPS_LATITUDE, "52/1,30/1,0/1")
                setAttribute(ExifInterface.TAG_GPS_LATITUDE_REF, "N")
                setAttribute(ExifInterface.TAG_MAKER_NOTE, "private maker note")
                if (xmp != null) setAttribute("Xmp", xmp)
                saveAttributes()
            }
            file.readBytes()
        } finally {
            file.delete()
        }
    }

    private val xmpHeader = "http://ns.adobe.com/xap/1.0/\u0000".toByteArray()
    private val xml = """
        <x:xmpmeta xmlns:x="adobe:ns:meta/">
          <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description rdf:about="" xmlns:t="http://ns.adobe.com/tiff/1.0/"
                xmlns:e="http://ns.adobe.com/exif/1.0/" xmlns:c="urn:exact:fixture"
                t:Orientation="6" t:ImageWidth="600" t:ImageLength="400" e:PixelXDimension="600"
                e:OffsetTimeOriginal="+02:00" t:StripOffsets="4096" t:JPEGInterchangeFormat="8192"
                c:Secret="private XMP location">
              <e:PixelYDimension>400</e:PixelYDimension>
              <e:OffsetTime>+01:00</e:OffsetTime>
              <e:OffsetTimeDigitized>+03:00</e:OffsetTimeDigitized>
              <c:Caption>Exact XMP caption</c:Caption>
            </rdf:Description>
          </rdf:RDF>
        </x:xmpmeta>
    """.trimIndent()

    private fun occurrences(bytes: ByteArray, text: String): Int =
        bytes.toString(Charsets.ISO_8859_1).windowed(text.length).count { it == text }

    @Test fun keepRetainsIptcOutsideExifThroughRepeatedEdits() {
        val rights = dataset(116, "Exact fixture copyright")
        val caption = dataset(120, "Exact fixture caption")
        val imageType = dataset(130, "01")
        val oldOrientation = dataset(131, "P")
        val preview = dataset(200, "old preview format") + dataset(201, "old preview version") +
            dataset(202, "obsolete thumbnail")
        var source = withMetadata(tagged(jpeg()), photoshop(rights + caption + imageType + oldOrientation + preview),
            segment(0xe1, xmpHeader + xml.toByteArray()))
        repeat(2) {
            val result = ImageCodec.metadata(source, jpeg(3, 2), "image/jpeg", "keep", 3, 2)
            assertNotNull(result)
            val bytes = result!!
            assertTrue("IPTC rights survive", bytes.toString(Charsets.ISO_8859_1).contains("Exact fixture copyright"))
            assertTrue("IPTC caption survives", bytes.toString(Charsets.ISO_8859_1).contains("Exact fixture caption"))
            assertEquals(1, occurrences(bytes, "Exact fixture copyright"))
            assertEquals(1, occurrences(bytes, "Exact XMP caption"))
            assertEquals(1, occurrences(bytes, "private XMP location"))
            assertEquals(0, occurrences(bytes, "obsolete thumbnail"))
            assertEquals(0, occurrences(bytes, "old preview"))
            assertEquals(1, occurrences(bytes, imageType.toString(Charsets.ISO_8859_1)))
            assertTrue(bytes.toString(Charsets.ISO_8859_1).contains(dataset(131, "L").toString(Charsets.ISO_8859_1)))
            val start = bytes.toString(Charsets.ISO_8859_1).indexOf(xmpHeader.toString(Charsets.ISO_8859_1))
            assertTrue("XMP stays outside EXIF", start >= 0)
            val length = ((bytes[start - 2].toInt() and 255) shl 8) or (bytes[start - 1].toInt() and 255)
            val packet = bytes.copyOfRange(start + xmpHeader.size, start + length - 2)
            val document = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
                .newDocumentBuilder().parse(ByteArrayInputStream(packet))
            val description = document.getElementsByTagNameNS("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Description").item(0)
            assertEquals("1", description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "Orientation").nodeValue)
            assertEquals("3", description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "ImageWidth").nodeValue)
            assertEquals("2", description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "ImageLength").nodeValue)
            assertEquals("3", description.attributes.getNamedItemNS("http://ns.adobe.com/exif/1.0/", "PixelXDimension").nodeValue)
            assertEquals("2", document.getElementsByTagNameNS("http://ns.adobe.com/exif/1.0/", "PixelYDimension").item(0).textContent)
            assertEquals("+02:00", description.attributes.getNamedItemNS("http://ns.adobe.com/exif/1.0/", "OffsetTimeOriginal").nodeValue)
            assertEquals("+01:00", document.getElementsByTagNameNS("http://ns.adobe.com/exif/1.0/", "OffsetTime").item(0).textContent)
            assertEquals("+03:00", document.getElementsByTagNameNS("http://ns.adobe.com/exif/1.0/", "OffsetTimeDigitized").item(0).textContent)
            assertNull(description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "StripOffsets"))
            assertNull(description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "JPEGInterchangeFormat"))
            val exif = ExifInterface(ByteArrayInputStream(bytes))
            assertEquals(1, exif.getAttributeInt(ExifInterface.TAG_ORIENTATION, 0))
            assertEquals(3, exif.getAttributeInt(ExifInterface.TAG_PIXEL_X_DIMENSION, 0))
            assertEquals(2, exif.getAttributeInt(ExifInterface.TAG_PIXEL_Y_DIMENSION, 0))
            assertNotNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
            assertEquals("Exact camera", exif.getAttribute(ExifInterface.TAG_MAKE))
            assertNotNull(exif.getAttribute(ExifInterface.TAG_MAKER_NOTE))
            assertCaptureOffsets(exif)
            val pixels = ImageIO.read(ByteArrayInputStream(bytes))
            assertEquals(3, pixels.width)
            assertEquals(2, pixels.height)
            source = bytes
        }
    }

    @Test fun keepRetainsExifXmpBesideASeparatePacket() {
        val embedded = """
            <x:xmpmeta xmlns:x="adobe:ns:meta/">
              <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
                <rdf:Description rdf:about="" xmlns:t="http://ns.adobe.com/tiff/1.0/" xmlns:c="urn:exact:fixture"
                    t:Orientation="6" c:Label="Exact EXIF XMP label"/>
              </rdf:RDF>
            </x:xmpmeta>
        """.trimIndent()
        val base = tagged(jpeg(), embedded)
        val packet = segment(0xe1, xmpHeader + xml.toByteArray())
        // ExifInterface reads whichever of the two packets it meets first, so place the separate one on each side.
        val sources = listOf(withMetadata(base, packet), base.copyOfRange(0, base.size - 2) + packet + base.takeLast(2).toByteArray())
        for (original in sources) {
            var source = original
            repeat(2) {
                source = ImageCodec.metadata(source, jpeg(3, 2), "image/jpeg", "keep", 3, 2)!!
                val tag = ExifInterface(ByteArrayInputStream(source)).getAttribute("Xmp")!!
                assertTrue("EXIF keeps its own packet", tag.contains("Exact EXIF XMP label"))
                assertFalse(tag.contains("Exact XMP caption"))
                val start = source.toString(Charsets.ISO_8859_1).indexOf(xmpHeader.toString(Charsets.ISO_8859_1))
                val length = ((source[start - 2].toInt() and 255) shl 8) or (source[start - 1].toInt() and 255)
                val separate = source.copyOfRange(start + xmpHeader.size, start + length - 2)
                assertEquals(1, occurrences(separate, "Exact XMP caption"))
                assertEquals(0, occurrences(separate, "Exact EXIF XMP label"))
                assertEquals(1, occurrences(source, xmpHeader.toString(Charsets.ISO_8859_1)))
                assertEquals(0, occurrences(source, "Orientation=\"6\""))
            }
        }
    }

    @Test fun noLocationKeepsOnlyCaptureTagsAndIptcRights() {
        val rights = dataset(80, "Exact byline") + dataset(110, "Exact credit") +
            dataset(115, "Exact source") + dataset(116, "Exact copyright")
        val location = dataset(90, "private city") + dataset(120, "private caption") + dataset(202, "private preview")
        var source = withMetadata(tagged(jpeg()), photoshop(rights + location), segment(0xe1, xmpHeader + xml.toByteArray()))
        repeat(2) {
            source = ImageCodec.metadata(source, jpeg(), "image/jpeg", "no-location", 6, 4)!!
            for (value in listOf("Exact byline", "Exact credit", "Exact source", "Exact copyright")) {
                assertEquals(value, 1, occurrences(source, value))
            }
            assertEquals(0, occurrences(source, "private"))
            assertEquals(0, occurrences(source, xmpHeader.toString(Charsets.ISO_8859_1)))
            val exif = ExifInterface(ByteArrayInputStream(source))
            assertNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
            assertNull(exif.getAttribute(ExifInterface.TAG_MAKER_NOTE))
            assertEquals("Exact camera", exif.getAttribute(ExifInterface.TAG_MAKE))
            assertEquals("2026:10:24 12:34:56", exif.getAttribute(ExifInterface.TAG_DATETIME_ORIGINAL))
            assertCaptureOffsets(exif)
        }
    }

    private fun assertCaptureOffsets(exif: ExifInterface) {
        assertEquals("+01:00", exif.getAttribute(ExifInterface.TAG_OFFSET_TIME))
        assertEquals("+02:00", exif.getAttribute(ExifInterface.TAG_OFFSET_TIME_ORIGINAL))
        assertEquals("+03:00", exif.getAttribute(ExifInterface.TAG_OFFSET_TIME_DIGITIZED))
    }

    @Test fun metadataAfterJpegScansSurvivesRepeatedEdits() {
        val rights = photoshop(dataset(116, "Exact late copyright"))
        val xmp = segment(0xe1, xmpHeader + xml.toByteArray())
        for (progressive in listOf(false, true)) {
            val base = tagged(jpeg(progressive = progressive))
            val late = base.copyOfRange(0, base.size - 2) + rights + xmp + base.takeLast(2).toByteArray()
            assertNotNull("the source JPEG decodes", ImageIO.read(ByteArrayInputStream(late)))
            for (mode in listOf("keep", "no-location")) {
                var source = late
                repeat(2) {
                    val result = ImageCodec.metadata(source, jpeg(3, 2), "image/jpeg", mode, 3, 2)
                    assertNotNull("$mode preserves metadata after the scan", result)
                    source = result!!
                    assertEquals(1, occurrences(source, "Exact late copyright"))
                    assertEquals(if (mode == "keep") 1 else 0, occurrences(source, "Exact XMP caption"))
                    assertNotNull(ImageIO.read(ByteArrayInputStream(source)))
                }
            }
            val rewritten = ImageCodec.metadata(late, late, "image/jpeg", "keep", 6, 4)!!
            assertEquals(1, occurrences(rewritten, "Exact late copyright"))
            assertEquals(1, occurrences(rewritten, "Exact XMP caption"))
            assertNotNull(ImageIO.read(ByteArrayInputStream(rewritten)))
            val truncated = base.copyOfRange(0, base.size - 2) + byteArrayOf(-1, 0xed.toByte(), 0, 20, 1)
            assertNull(ImageCodec.metadata(truncated, jpeg(), "image/jpeg", "keep", 6, 4))
            assertNull(ImageCodec.metadata(base.copyOf(base.size - 2), jpeg(), "image/jpeg", "keep", 6, 4))
        }
    }

    @Test fun rewritingSegmentsBetweenScansLeavesEscapedEntropyIntact() {
        val soi = byteArrayOf(-1, -40)
        val eoi = byteArrayOf(-1, -39)
        val scan = segment(0xda, byteArrayOf(1, 1, 0, 0, 63, 0))
        val entropy = byteArrayOf(12, -1, 0, 0xed.toByte(), 0, 2, -1, 0xd0.toByte(), 34, -1, -1, 0xd7.toByte(), 56)
        val source = soi + scan + entropy + photoshop(dataset(116, "late rights")) +
            scan + entropy + segment(0xe1, xmpHeader + xml.toByteArray()) + eoi
        assertArrayEquals(soi + scan + entropy + scan + entropy + eoi,
            ImageMetadata.write(source, "image/jpeg", emptyList()))
    }

    @Test fun embeddedExifXmpSurvivesPngAndWebpEdits() {
        for (original in listOf(png(), webp())) {
            val tagged = tagged(original, xml)
            assertEquals(xml, ExifInterface(ByteArrayInputStream(tagged)).getAttribute("Xmp"))
            assertEquals(0, occurrences(tagged, "XML:com.adobe.xmp"))
            val xmpChunk = "XMP ".toByteArray() + intBytes(xml.toByteArray().size, java.nio.ByteOrder.LITTLE_ENDIAN)
            assertEquals(0, occurrences(tagged, xmpChunk.toString(Charsets.ISO_8859_1)))
            for ((type, encoded) in listOf("image/jpeg" to jpeg(3, 2), "image/png" to png(), "image/webp" to webp())) {
                val width = if (type == "image/webp") 1 else 3
                val height = if (type == "image/webp") 1 else 2
                for (mode in listOf("keep", "no-location")) {
                    var source = tagged
                    repeat(2) {
                        val result = ImageCodec.metadata(source, encoded, type, mode, width, height)
                        assertNotNull("embedded XMP with $mode to $type", result)
                        source = result!!
                        val exif = ExifInterface(ByteArrayInputStream(source))
                        assertEquals("Exact camera", exif.getAttribute(ExifInterface.TAG_MAKE))
                        assertCaptureOffsets(exif)
                        val packet = exif.getAttribute("Xmp")
                        if (mode == "no-location") {
                            assertNull(packet)
                            assertEquals(0, occurrences(source, "private"))
                        } else {
                            assertNotNull(packet)
                            assertEquals(1, occurrences(packet!!.toByteArray(), "Exact XMP caption"))
                            val document = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
                                .newDocumentBuilder().parse(ByteArrayInputStream(packet.toByteArray()))
                            val description = document.getElementsByTagNameNS("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Description").item(0)
                            assertEquals("1", description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "Orientation").nodeValue)
                            assertEquals(width.toString(), description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "ImageWidth").nodeValue)
                            assertEquals(height.toString(), document.getElementsByTagNameNS("http://ns.adobe.com/exif/1.0/", "PixelYDimension").item(0).textContent)
                            assertNull(description.attributes.getNamedItemNS("http://ns.adobe.com/tiff/1.0/", "StripOffsets"))
                        }
                    }
                }
            }
        }
    }

    @Test fun otherSourceContainersKeepReadableExif() {
        val jpeg = tagged(jpeg())
        val start = jpeg.toString(Charsets.ISO_8859_1).indexOf("Exif\u0000\u0000")
        val size = ((jpeg[start - 2].toInt() and 255) shl 8) or (jpeg[start - 1].toInt() and 255)
        // A bare TIFF directory exercises ExifInterface's RAW input path without a RAW pixel decoder.
        val tiff = jpeg.copyOfRange(start + 6, start + size - 2)
        assertEquals("Exact camera", ExifInterface(ByteArrayInputStream(tiff)).getAttribute(ExifInterface.TAG_MAKE))
        for (mode in listOf("keep", "no-location")) {
            var source = tiff
            repeat(2) {
                val result = ImageCodec.metadata(source, jpeg(3, 2), "image/jpeg", mode, 3, 2)
                assertNotNull("$mode preserves readable TIFF EXIF", result)
                source = result!!
                val exif = ExifInterface(ByteArrayInputStream(source))
                assertEquals("Exact camera", exif.getAttribute(ExifInterface.TAG_MAKE))
                assertCaptureOffsets(exif)
                assertEquals(1, exif.getAttributeInt(ExifInterface.TAG_ORIENTATION, 0))
                assertEquals(3, exif.getAttributeInt(ExifInterface.TAG_PIXEL_X_DIMENSION, 0))
                assertEquals(2, exif.getAttributeInt(ExifInterface.TAG_PIXEL_Y_DIMENSION, 0))
                if (mode == "keep") assertNotNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
                else assertNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
            }
        }
    }

    @Test fun pngAndWebpSourcesKeepExifThroughRepeatedEdits() {
        for (original in listOf(png(), webp())) {
            val tagged = tagged(original)
            assertCaptureOffsets(ExifInterface(ByteArrayInputStream(tagged)))
            for (mode in listOf("keep", "no-location")) {
                for ((type, encoded) in listOf("image/jpeg" to jpeg(3, 2), "image/png" to png(), "image/webp" to webp())) {
                    val width = if (type == "image/webp") 1 else 3
                    val height = if (type == "image/webp") 1 else 2
                    var source = tagged
                    repeat(2) {
                        val result = ImageCodec.metadata(source, encoded, type, mode, width, height)
                        assertNotNull("$mode to $type preserves EXIF", result)
                        source = result!!
                        val exif = ExifInterface(ByteArrayInputStream(source))
                        assertEquals("Exact camera", exif.getAttribute(ExifInterface.TAG_MAKE))
                        assertEquals("2026:10:24 12:34:56", exif.getAttribute(ExifInterface.TAG_DATETIME_ORIGINAL))
                        assertEquals(1, exif.getAttributeInt(ExifInterface.TAG_ORIENTATION, 0))
                        assertEquals(width, exif.getAttributeInt(ExifInterface.TAG_PIXEL_X_DIMENSION, 0))
                        assertEquals(height, exif.getAttributeInt(ExifInterface.TAG_PIXEL_Y_DIMENSION, 0))
                        assertCaptureOffsets(exif)
                        if (mode == "keep") assertNotNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
                        else assertNull(exif.getAttribute(ExifInterface.TAG_GPS_LATITUDE))
                    }
                }
            }
        }
    }

    @Test fun unsupportedXmpGeometryCannotReportKeep() {
        for (property in listOf(
            """<t:Orientation rdf:parseType="Resource"><rdf:value>6</rdf:value><c:Confidence>high</c:Confidence></t:Orientation>""",
            """<e:PixelXDimension><rdf:Description rdf:value="600" c:Confidence="high"/></e:PixelXDimension>""",
            """<t:Orientation rdf:resource="urn:exact:orientation:6"/>""",
            """<t:Orientation rdf:value="6" c:Confidence="high"/>""",
        )) {
            val packet = xml.replace(" t:Orientation=\"6\"", "").replace(" e:PixelXDimension=\"600\"", "")
                .replace("<e:PixelYDimension>400</e:PixelYDimension>", property)
            val source = withMetadata(jpeg(), segment(0xe1, xmpHeader + packet.toByteArray()))
            assertNull(property, ImageCodec.metadata(source, jpeg(), "image/jpeg", "keep", 6, 4))
            assertNotNull(ImageCodec.metadata(source, jpeg(), "image/jpeg", "no-location", 6, 4))
        }
    }

    @Test fun nonJpegExternalMetadataIsReportedUnsupported() {
        val pngXmp = pngText(tagged(png()), "iTXt", "XML:com.adobe.xmp\u0000\u0000\u0000\u0000\u0000" + xml)
        for (source in listOf(pngXmp, webpXmp(tagged(webp())))) {
            assertNull(ImageCodec.metadata(source, jpeg(), "image/jpeg", "keep", 6, 4))
            val stripped = ImageCodec.metadata(source, jpeg(), "image/jpeg", "no-location", 6, 4)!!
            assertEquals(0, occurrences(stripped, "private"))
            assertEquals("Exact camera", ExifInterface(ByteArrayInputStream(stripped)).getAttribute(ExifInterface.TAG_MAKE))
        }
        val iptc = pngText(tagged(png()), "tEXt", "Raw profile type iptc\u0000Exact rights")
        assertNull(ImageCodec.metadata(iptc, jpeg(), "image/jpeg", "keep", 6, 4))
        assertNull(ImageCodec.metadata(iptc, jpeg(), "image/jpeg", "no-location", 6, 4))
    }

    @Test fun theSharedEveryTagFixtureKeepsItsIptcAndPaddedXmp() {
        val source = File("../../../scripts/fixtures/picker/every-tag.jpg").readBytes()
        val copyright = dataset(116, "(c) Exact").toString(Charsets.ISO_8859_1)
        val kept = ImageCodec.metadata(source, jpeg(), "image/jpeg", "keep", 6, 4)!!
        assertEquals(1, occurrences(kept, "xmp-value"))
        assertEquals(1, occurrences(kept, copyright))
        assertEquals(1, occurrences(kept, "8BIM"))
        val private = ImageCodec.metadata(source, jpeg(), "image/jpeg", "no-location", 6, 4)!!
        assertEquals(0, occurrences(private, "xmp-value"))
        assertEquals(1, occurrences(private, copyright))
        assertEquals(1, occurrences(private, "8BIM"))
    }

    @Test fun malformedAndUnsupportedPreservationReturnsNone() {
        val base = jpeg()
        val extended = segment(0xe1, "http://ns.adobe.com/xmp/extension/\u0000unhandled".toByteArray())
        val malformed = listOf(
            byteArrayOf(), base.copyOf(12), png().copyOf(24), webp().copyOf(20),
            withMetadata(base, byteArrayOf(-1, 0xed.toByte(), 0xff.toByte(), 0xff.toByte())),
            withMetadata(base, photoshop(byteArrayOf(0x1c, 2, 116, 0, 50, 0))),
            withMetadata(base, segment(0xed, "Photoshop 3.0\u00008BIM".toByteArray())),
            withMetadata(base, segment(0xe1, xmpHeader + "<broken>".toByteArray())),
            withMetadata(base, segment(0xe1, xmpHeader + "<!DOCTYPE x [<!ENTITY s 'secret'>]><x>&s;</x>".toByteArray())),
            withMetadata(base, extended),
        )
        for ((i, source) in malformed.withIndex()) {
            assertNull("fixture $i cannot report keep", ImageCodec.metadata(source, base, "image/jpeg", "keep", 6, 4))
        }
        assertNotNull(ImageCodec.metadata(withMetadata(base, extended), base, "image/jpeg", "no-location", 6, 4))
        val source = withMetadata(base, photoshop(dataset(116, "rights")))
        assertNull(ImageCodec.metadata(source, png(), "image/png", "keep", 3, 2))
        assertNull(ImageCodec.metadata(source, base, "image/jpeg", "none", 6, 4))
        val multiPicture = withMetadata(base, segment(0xe2, "MPF\u0000".toByteArray()))
        assertNull(ImageCodec.metadata(multiPicture, base, "image/jpeg", "keep", 6, 4))
        assertNull(ImageCodec.metadata(base, multiPicture, "image/jpeg", "keep", 6, 4))
        val unknownResource = "Photoshop 3.0\u00008BIM".toByteArray() + byteArrayOf(4, 36, 0, 0, 0, 0, 0, 0)
        val unknown = withMetadata(base, segment(0xed, unknownResource))
        assertNull(ImageCodec.metadata(unknown, base, "image/jpeg", "keep", 6, 4))
        assertNotNull(ImageCodec.metadata(unknown, base, "image/jpeg", "no-location", 6, 4))
    }
}
