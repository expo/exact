package dev.exact.canvas

import java.io.ByteArrayOutputStream
import java.io.StringReader
import javax.xml.parsers.DocumentBuilderFactory
import javax.xml.transform.OutputKeys
import javax.xml.transform.TransformerFactory
import javax.xml.transform.dom.DOMSource
import javax.xml.transform.stream.StreamResult
import org.w3c.dom.Element
import org.xml.sax.InputSource

/** IPTC/XMP preservation (LLP 1133.019 D15). External PNG/WebP metadata and extended XMP are unsupported. */
internal object ImageMetadata {
    private val photoshop = "Photoshop 3.0\u0000".toByteArray()
    private val xmp = "http://ns.adobe.com/xap/1.0/\u0000".toByteArray()
    private val extended = "http://ns.adobe.com/xmp/extension/\u0000".toByteArray()
    private val mpf = "MPF\u0000".toByteArray()
    private const val tiff = "http://ns.adobe.com/tiff/1.0/"
    private const val exif = "http://ns.adobe.com/exif/1.0/"
    private const val note = "http://ns.adobe.com/xmp/note/"
    private const val rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"

    private fun ByteArray.u8(at: Int) = this[at].toInt() and 255
    private fun ByteArray.u16(at: Int) = (u8(at) shl 8) or u8(at + 1)
    private fun ByteArray.starts(prefix: ByteArray) = size >= prefix.size && prefix.indices.all { this[it] == prefix[it] }
    private data class Segment(val marker: Int, val start: Int, val end: Int, val payload: ByteArray)
    data class Contents(val segments: List<ByteArray>, val embeddedXmp: String?)

    /** Metadata may follow a scan. Walk through the first EOI, skipping escaped bytes and restart markers. */
    private fun segments(bytes: ByteArray): List<Segment> {
        require(bytes.size >= 2 && bytes.u16(0) == 0xffd8)
        val segments = ArrayList<Segment>()
        var at = 2
        var scanning = false
        while (at < bytes.size) {
            if (scanning && bytes.u8(at) != 255) {
                at++
                continue
            }
            val start = at
            require(bytes.u8(at++) == 255)
            while (at < bytes.size && bytes.u8(at) == 255) at++
            require(at < bytes.size)
            val marker = bytes.u8(at++)
            if (marker == 0 || marker in 0xd0..0xd7) {
                require(scanning)
                continue
            }
            if (marker == 1) continue
            if (marker == 0xd9) return segments
            require(marker != 0xd8)
            require(at + 2 <= bytes.size)
            val size = bytes.u16(at)
            require(size >= 2 && size <= bytes.size - at)
            if (marker != 0xda) segments.add(Segment(marker, start, at + size, bytes.copyOfRange(at + 2, at + size)))
            // DNL supplies the line count inside a scan; its entropy data continues after the segment.
            scanning = marker == 0xda || (scanning && marker == 0xdc)
            at += size
        }
        throw IllegalArgumentException("incomplete JPEG")
    }

    private fun segment(marker: Int, payload: ByteArray): ByteArray {
        require(payload.size <= 65533)
        val size = payload.size + 2
        return byteArrayOf(-1, marker.toByte(), (size shr 8).toByte(), size.toByte()) + payload
    }

    /** Inspect PNG/WebP's external metadata; leave other containers to ExifInterface's attribute reader. */
    private fun nonJpeg(source: ByteArray, mode: String) {
        val png = source.starts(byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10))
        val webp = source.starts("RIFF".toByteArray()) && source.size >= 12 &&
            source.copyOfRange(8, 12).contentEquals("WEBP".toByteArray())
        if (!png && !webp) return
        var at = if (png) 8 else 12
        val overhead = if (png) 12 else 8
        while (at < source.size) {
            require(source.size - at >= overhead)
            var length = 0L
            for (i in 0..3) length = (length shl 8) or source.u8(at + if (png) i else 7 - i).toLong()
            require(length <= source.size - at - overhead)
            val nameAt = at + if (png) 4 else 0
            val name = source.copyOfRange(nameAt, nameAt + 4).toString(Charsets.ISO_8859_1)
            if (png && name in setOf("tEXt", "zTXt", "iTXt")) {
                val data = source.copyOfRange(at + 8, at + 8 + length.toInt())
                // A textual chunk may carry IPTC rights. Only a known XMP packet is safe to discard.
                require(mode == "no-location" && data.starts("XML:com.adobe.xmp\u0000".toByteArray()))
            } else if (!png && name == "XMP ") {
                require(mode == "no-location")
            }
            at += overhead + length.toInt()
            if (!png) at += length.toInt() and 1
            require(at <= source.size)
        }
    }

    /**
     * [exifXmp] is `ExifInterface`'s XMP attribute: IFD0's tag 700 when there is one, else a JPEG's separate packet.
     * A tag that differs from the separate packet is kept beside it, as two copies, rather than merged.
     */
    fun read(source: ByteArray, exifXmp: ByteArray?, mode: String, width: Int, height: Int): Contents {
        require(mode == "keep" || mode == "no-location")
        require(source.isNotEmpty())
        val out = ArrayList<ByteArray>()
        var externalXmp: ByteArray? = null
        val segments = if (source.starts(byteArrayOf(-1, -40))) segments(source) else {
            nonJpeg(source, mode)
            emptyList()
        }
        for (s in segments) {
            if (s.marker == 0xe2 && s.payload.starts(mpf)) require(mode != "keep")
            if (s.marker == 0xed) {
                require(s.payload.starts(photoshop))
                iptc(s.payload, mode, width, height)?.let { out.add(segment(0xed, photoshop + it)) }
            } else if (s.marker == 0xe1 && s.payload.starts(extended)) {
                require(mode != "keep")
            } else if (s.marker == 0xe1 && s.payload.starts(xmp)) {
                require(externalXmp == null)
                val packet = s.payload.copyOfRange(xmp.size, s.payload.size)
                externalXmp = packet
                if (mode == "keep") {
                    val xml = packet.decodeToString(throwOnInvalidSequence = true)
                    out.add(segment(0xe1, xmp + uprightXmp(xml, width, height)))
                }
            }
        }
        val embedded = if (mode == "keep" && exifXmp != null && !exifXmp.contentEquals(externalXmp)) {
            uprightXmp(exifXmp.decodeToString(), width, height).toString(Charsets.UTF_8)
        } else null
        return Contents(out, embedded)
    }

    /** Keep only IPTC image resources, not Photoshop's thumbnails, EXIF, digests or document geometry. */
    private fun iptc(bytes: ByteArray, mode: String, width: Int, height: Int): ByteArray? {
        val out = ByteArrayOutputStream()
        var at = photoshop.size
        while (at < bytes.size) {
            require(bytes.size - at >= 12)
            require(bytes.copyOfRange(at, at + 4).contentEquals("8BIM".toByteArray()))
            val id = bytes.u16(at + 4)
            at += 6
            val nameSize = (bytes.u8(at) + 2) and -2
            require(nameSize <= bytes.size - at - 4)
            at += nameSize
            var length = 0L
            repeat(4) { length = (length shl 8) or bytes.u8(at++).toLong() }
            require(length <= bytes.size - at)
            val end = at + length.toInt()
            if (id == 0x0404) {
                val data = datasets(bytes.copyOfRange(at, end), mode, width, height)
                if (data.isNotEmpty()) {
                    out.write("8BIM".toByteArray())
                    out.write(byteArrayOf(4, 4, 0, 0))
                    out.write(byteArrayOf(0, 0, (data.size shr 8).toByte(), data.size.toByte()))
                    out.write(data)
                    if (data.size % 2 != 0) out.write(0)
                }
            } else if (mode == "keep") {
                // Thumbnails and the IPTC digest describe the old pixels or old IPTC bytes. Other resources may
                // contain metadata we cannot preserve, including Photoshop's own EXIF and XMP resources.
                require(id in setOf(0x0409, 0x040c, 0x0425))
            }
            at = end + (length.toInt() and 1)
            require(at <= bytes.size)
        }
        return out.toByteArray().takeIf { it.isNotEmpty() }
    }

    private fun datasets(bytes: ByteArray, mode: String, width: Int, height: Int): ByteArray {
        val out = ByteArrayOutputStream()
        var at = 0
        while (at < bytes.size) {
            val start = at
            require(bytes.size - at >= 5 && bytes.u8(at) == 0x1c)
            val record = bytes.u8(at + 1)
            val field = bytes.u8(at + 2)
            var length = bytes.u16(at + 3).toLong()
            at += 5
            if (length and 0x8000 != 0L) {
                val count = (length and 0x7fff).toInt()
                require(count in 1..4 && count <= bytes.size - at)
                length = 0
                repeat(count) { length = (length shl 8) or bytes.u8(at++).toLong() }
            }
            require(length <= bytes.size - at)
            val end = at + length.toInt()
            if (mode == "keep") {
                require(record in 1..2)
                when {
                    record == 2 && field == 131 -> {
                        val orientation = if (width == height) 'S' else if (width > height) 'L' else 'P'
                        out.write(byteArrayOf(0x1c, 2, 131.toByte(), 0, 1, orientation.code.toByte()))
                    }
                    record == 2 && field in setOf(200, 201, 202) -> Unit
                    else -> out.write(bytes, start, end - start)
                }
            } else if ((record == 2 && field in setOf(0, 80, 110, 115, 116)) || (record == 1 && field == 90)) {
                // IIM 2:80 By-line, 2:110 Credit, 2:115 Source, 2:116 Copyright Notice; plus version and charset.
                out.write(bytes, start, end - start)
            }
            at = end
        }
        return out.toByteArray()
    }

    private fun uprightXmp(xml: String, width: Int, height: Int): ByteArray {
        // A JPEG XMP packet is UTF-8 XML. Refuse DTDs before parsing, including internal entity declarations.
        require(!xml.contains("<!DOCTYPE"))
        val builder = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }.newDocumentBuilder()
        builder.setEntityResolver { _, _ -> throw IllegalArgumentException("external XMP entity") }
        val document = builder.parse(InputSource(StringReader(xml.trimEnd('\u0000'))))
        fun value(namespace: String?, name: String?): String? {
            if (namespace == note && name == "HasExtendedXMP") throw IllegalArgumentException("extended XMP")
            if (namespace != tiff && namespace != exif) return null
            return when (name) {
                "Orientation" -> "1"
                "ImageWidth", "PixelXDimension" -> width.toString()
                "ImageLength", "PixelYDimension" -> height.toString()
                else -> if (name != null && ImageCodec.structural(name)) "" else null
            }
        }
        fun visit(element: Element) {
            for (i in element.attributes.length - 1 downTo 0) {
                val a = element.attributes.item(i)
                value(a.namespaceURI, a.localName)?.let {
                    if (it.isEmpty()) element.removeAttributeNode(a as org.w3c.dom.Attr) else a.nodeValue = it
                }
            }
            var child = element.firstChild
            while (child != null) {
                val next = child.nextSibling
                if (child is Element) {
                    val replacement = value(child.namespaceURI, child.localName)
                    when (replacement) {
                        null -> visit(child)
                        "" -> element.removeChild(child)
                        else -> {
                            // Resource and qualified RDF values need a structural rewrite, not textContent.
                            require((0 until child.childNodes.length).none { child.childNodes.item(it) is Element })
                            require((0 until child.attributes.length).none {
                                val a = child.attributes.item(it)
                                a.namespaceURI == rdf && a.localName != "datatype"
                            })
                            child.textContent = replacement
                        }
                    }
                }
                child = next
            }
        }
        visit(document.documentElement)
        val out = ByteArrayOutputStream()
        TransformerFactory.newInstance().newTransformer().apply {
            setOutputProperty(OutputKeys.ENCODING, "UTF-8")
            setOutputProperty(OutputKeys.OMIT_XML_DECLARATION, "yes")
        }.transform(DOMSource(document), StreamResult(out))
        return out.toByteArray()
    }

    fun write(encoded: ByteArray, type: String, segments: List<ByteArray>): ByteArray {
        if (type != "image/jpeg") return encoded
        val existing = segments(encoded)
        // Adding header bytes can invalidate a multi-picture JPEG's offsets, including an HDR gain map's.
        require(existing.none { it.marker == 0xe2 && it.payload.starts(mpf) })
        val out = ByteArrayOutputStream()
        out.write(encoded, 0, 2)
        segments.forEach { out.write(it) }
        var at = 2
        for (s in existing) {
            val external = (s.marker == 0xed && s.payload.starts(photoshop)) ||
                (s.marker == 0xe1 && (s.payload.starts(xmp) || s.payload.starts(extended)))
            if (external) {
                out.write(encoded, at, s.start - at)
                at = s.end
            }
        }
        out.write(encoded, at, encoded.size - at)
        return out.toByteArray()
    }
}
