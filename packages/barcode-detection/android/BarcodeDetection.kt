// ML Kit's bundled detector for stills and non-consuming camera scans (LLP 1133.027 D4, D5, D14).
package exact.barcodedetection

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import android.os.Handler
import android.os.Looper
import android.util.Base64
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.barcode.BarcodeScanner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.TrackError
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

class BarcodeDetectionModule(context: ExtensionContext) : ExactExtension(context) {
    internal val work = Executors.newSingleThreadExecutor { Thread(it, "exact.barcode-detection") }
    internal val main = Handler(Looper.getMainLooper())
    private val scans = HashMap<String, Scan>()
    private val groups = HashMap<String, MutableMap<String, Double>>()
    private val stills = ArrayDeque<JSONObject>()

    internal fun say(line: String) = context.log("barcode-detection: $line")

    internal fun disclose() {
        if (!context.agent) return
        val active = scans.toSortedMap().flatMap { (key, scan) ->
            groups[key].orEmpty().values.distinct().sorted().map(scan::disclosure)
        }
        context.state(JSONObject().put("scans", JSONArray(active)).put("stills", JSONArray(stills.toList())))
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        when (name) {
            "getSupportedFormats" -> reply(supported())
            "detectBarcodes" -> still(args, reply)
            "detectedBarcodes" -> scan(args, reply)
            else -> reply(JSONObject().put("error", "NotSupportedError"))
        }
    }

    // Formats (D1, D11)

    companion object {
        val formats = listOf("aztec", "code_128", "code_39", "code_93", "codabar", "data_matrix", "ean_13", "ean_8", "itf", "pdf417", "qr_code", "upc_a", "upc_e")

        private val byName = mapOf(
            "aztec" to Barcode.FORMAT_AZTEC, "code_128" to Barcode.FORMAT_CODE_128, "code_39" to Barcode.FORMAT_CODE_39,
            "code_93" to Barcode.FORMAT_CODE_93, "codabar" to Barcode.FORMAT_CODABAR, "data_matrix" to Barcode.FORMAT_DATA_MATRIX,
            "ean_13" to Barcode.FORMAT_EAN_13, "ean_8" to Barcode.FORMAT_EAN_8, "itf" to Barcode.FORMAT_ITF,
            "pdf417" to Barcode.FORMAT_PDF417, "qr_code" to Barcode.FORMAT_QR_CODE, "upc_a" to Barcode.FORMAT_UPC_A,
            "upc_e" to Barcode.FORMAT_UPC_E,
        )

        /** The web's name for an ML Kit format, or `unknown`. */
        fun format(f: Int): String = byName.entries.firstOrNull { it.value == f }?.key ?: "unknown"

        private val valueTypes = mapOf(
            Barcode.TYPE_CONTACT_INFO to "contact_info", Barcode.TYPE_EMAIL to "email", Barcode.TYPE_ISBN to "isbn",
            Barcode.TYPE_PHONE to "phone", Barcode.TYPE_PRODUCT to "product", Barcode.TYPE_SMS to "sms",
            Barcode.TYPE_TEXT to "text", Barcode.TYPE_URL to "url", Barcode.TYPE_WIFI to "wifi", Barcode.TYPE_GEO to "geo",
            Barcode.TYPE_CALENDAR_EVENT to "calendar_event", Barcode.TYPE_DRIVER_LICENSE to "driver_license",
        )

        fun supported(): JSONObject = JSONObject().put("formats", JSONArray(formats)).put("tracks", true)
            .put("android", JSONObject().put("bundled", true))

        /** The formats asked: null for all; or D2's `TypeError` as the error. */
        fun asked(value: Any?): Pair<Set<String>?, JSONObject?> {
            if (value == null || value == JSONObject.NULL) return null to null
            val list = (value as? JSONArray)?.let { a -> (0 until a.length()).map { a.optString(it) } }
            if (list == null || list.isEmpty() || !list.all(formats::contains)) {
                return null to JSONObject().put("error", "TypeError").put("message", "formats name D1's barcode formats")
            }
            return list.toSet() to null
        }

        /** Ask both UPC shapes whenever either is requested, before normalising and filtering (D11). */
        fun requestedFormats(keep: Set<String>?): List<Int>? = keep?.let {
            if ("upc_a" in it || "ean_13" in it) it + setOf("upc_a", "ean_13") else it
        }?.mapNotNull(byName::get)

        fun scanner(keep: Set<String>?): BarcodeScanner {
            val want = requestedFormats(keep)
            val options = BarcodeScannerOptions.Builder()
            if (want.isNullOrEmpty()) options.setBarcodeFormats(Barcode.FORMAT_ALL_FORMATS)
            else options.setBarcodeFormats(want.first(), *want.drop(1).toIntArray())
            return BarcodeScanning.getClient(options.build())
        }

        /** One ML Kit barcode, in the upright image's pixels, its corners clockwise from the top left. */
        fun shape(b: Barcode): JSONObject {
            val box = b.boundingBox
            val android = JSONObject().put("valueType", valueTypes[b.valueType] ?: "unknown")
            b.rawBytes?.let { android.put("rawBytes", Base64.encodeToString(it, Base64.NO_WRAP)) }
            return JSONObject()
                .put("rawValue", b.rawValue ?: "")
                .put("format", format(b.format))
                .put("boundingBox", JSONObject().put("x", box?.left ?: 0).put("y", box?.top ?: 0)
                    .put("width", box?.width() ?: 0).put("height", box?.height() ?: 0))
                .put("cornerPoints", JSONArray((b.cornerPoints ?: emptyArray()).map { JSONObject().put("x", it.x).put("y", it.y) }))
                .put("android", android)
        }

        /** D11: one UPC-A answered as the formats asked name it, and only formats asked kept. */
        fun normalised(found: List<JSONObject>, keep: Set<String>?): List<JSONObject> = found.mapNotNull { r ->
            var format = r.optString("format", "unknown")
            var raw = r.optString("rawValue")
            val digits = raw.isNotEmpty() && raw.all { it in '0'..'9' }
            if (digits && ((format == "upc_a" && raw.length == 12) || (format == "ean_13" && raw.length == 13 && raw.startsWith("0")))) {
                val twelve = if (format == "upc_a") raw else raw.drop(1)
                if (keep == null || "upc_a" in keep) { format = "upc_a"; raw = twelve } else { format = "ean_13"; raw = "0$twelve" }
            }
            if (keep != null && format !in keep) return@mapNotNull null
            JSONObject(r.toString()).put("format", format).put("rawValue", raw)
        }
    }

    // Stills (D4)

    private fun still(args: JSONObject, reply: (JSONObject) -> Unit) {
        val (keep, refused) = asked(args.opt("formats"))
        if (refused != null) return reply(refused)
        val path = args.optString("image")
        val file = context.fileToRead(path) ?: return reply(JSONObject().put("error", "NotFoundError"))
        work.execute {
            val answer = try {
                val image = upright(file.path)
                if (image == null) JSONObject().put("error", "InvalidStateError").put("message", "decode")
                else {
                    try {
                        val client = scanner(keep)
                        try {
                            val found = Tasks.await(client.process(InputImage.fromBitmap(image, 0))).map(::shape)
                            JSONObject().put("barcodes", JSONArray(normalised(found, keep))).put("width", image.width).put("height", image.height)
                        } finally { client.close() }
                    } finally { image.recycle() }
                }
            } catch (e: Throwable) {
                JSONObject().put("error", "OperationError").put("message", e.message ?: "ML Kit failed")
            }
            main.post {
                answer.optJSONArray("barcodes")?.let {
                    say("$path: ${it.length()} found")
                    if (context.agent) {
                        stills.addLast(JSONObject().put("path", path).put("count", it.length()))
                        if (stills.size > 16) stills.removeFirst()
                        disclose()
                    }
                }
                reply(answer)
            }
        }
    }

    /** The file decoded with its EXIF orientation applied, or null when it does not decode. */
    internal fun upright(path: String): Bitmap? {
        val bitmap = BitmapFactory.decodeFile(path) ?: return null
        try {
            val orientation = runCatching { ExifInterface(path).getAttributeInt(ExifInterface.TAG_ORIENTATION, 1) }.getOrDefault(1)
            val m = Matrix()
            when (orientation) {
                ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> m.setScale(-1f, 1f)
                ExifInterface.ORIENTATION_ROTATE_180 -> m.setRotate(180f)
                ExifInterface.ORIENTATION_FLIP_VERTICAL -> m.setScale(1f, -1f)
                ExifInterface.ORIENTATION_TRANSPOSE -> { m.setRotate(90f); m.postScale(-1f, 1f) }
                ExifInterface.ORIENTATION_ROTATE_90 -> m.setRotate(90f)
                ExifInterface.ORIENTATION_TRANSVERSE -> { m.setRotate(-90f); m.postScale(-1f, 1f) }
                ExifInterface.ORIENTATION_ROTATE_270 -> m.setRotate(-90f)
                else -> return bitmap
            }
            return Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, m, true).also {
                if (it !== bitmap) bitmap.recycle()
            }
        } catch (e: Throwable) {
            bitmap.recycle()
            throw e
        }
    }

    // Live scans (D5)

    private fun scan(args: JSONObject, reply: (JSONObject) -> Unit) {
        val (keep, refused) = asked(args.opt("formats"))
        if (refused != null) return reply(refused)
        val track = args.optString("track")
        // No track yet (an app's state before its camera opens) is a scan with nothing to read.
        if (track.isEmpty()) {
            return reply(JSONObject().put("state", "idle").put("barcodes", JSONArray()).put("seen", JSONArray())
                .put("frame", 0).put("width", 0).put("height", 0))
        }
        val entry = context.tracks.lookup(track)
            ?: return reply(JSONObject().put("error", "NotFoundError").put("message", "the session holds no track $track"))
        if (entry.kind != "video") return reply(JSONObject().put("error", "NotSupportedError").put("message", "$track is not a video track"))
        val key = Scan.key(track, keep)
        if (scans[key] == null) {
            val s = Scan(this, track, keep)
            try { s.start() } catch (e: Exception) {
                s.stop()
                return reply(JSONObject().put("error", (e as? TrackError)?.code ?: "OperationError").put("message", e.message))
            }
            scans[key] = s
            say("scan $key: observing $track")
        }
        val interval = maxOf(50.0, args.optDouble("interval", 100.0))
        if (groups.getOrPut(key) { HashMap() }.put(argumentGroup(args), interval) == null) disclose()
        scans[key]?.analyse(reply)
    }

    override fun release(name: String, args: JSONObject) {
        if (name != "detectedBarcodes") return
        val track = args.optString("track").ifEmpty { return }
        val key = Scan.key(track, asked(args.opt("formats")).first)
        val active = groups[key] ?: return
        active.remove(argumentGroup(args))
        if (active.isEmpty()) end(key, "released") else disclose()
    }

    // Object property order is immaterial; the original array order and interval identify a runner group.
    private fun argumentGroup(value: Any?): String = when (value) {
        is JSONObject -> value.keys().asSequence().toList().sorted().joinToString(",", "{", "}") {
            JSONObject.quote(it) + ":" + argumentGroup(value.opt(it))
        }
        is JSONArray -> (0 until value.length()).joinToString(",", "[", "]") { argumentGroup(value.opt(it)) }
        is String -> JSONObject.quote(value)
        else -> value?.toString() ?: "null"
    }

    override fun orientation(angle: Int) { scans.values.forEach { it.orientation(angle) } }

    override fun release(name: String) {
        if (name == "detectedBarcodes") scans.keys.toList().forEach { end(it, "released") }
    }

    private fun end(key: String, why: String) {
        groups.remove(key)
        val s = scans.remove(key) ?: return
        s.stop()
        say("scan $key: $why")
        disclose()
    }

    override fun destroy(done: () -> Unit) {
        scans.keys.toList().forEach { end(it, "the session ended") }
        work.shutdown()
        done()
    }
}
