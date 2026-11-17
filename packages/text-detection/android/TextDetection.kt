// ML Kit's bundled Latin recognizer on still images (LLP 1133.027 D2, D4, D6, D14).
package exact.textdetection

import android.content.ComponentCallbacks2
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import android.os.Handler
import android.os.Looper
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.Text
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.TextRecognizer
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors

class TextDetectionModule(context: ExtensionContext) : ExactExtension(context) {
    internal val work = Executors.newSingleThreadExecutor { Thread(it, "exact.text-detection") }
    private val main = Handler(Looper.getMainLooper())
    private var recognizer: TextRecognizer? = null // Owned by work, including its disposal.
    private var watchingMemory = false
    private val memory = object : ComponentCallbacks2 {
        override fun onConfigurationChanged(configuration: Configuration) {}
        override fun onLowMemory() = discard()
        override fun onTrimMemory(level: Int) {
            if (level != ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN) discard()
        }
        private fun discard() { work.execute { recognizer?.close(); recognizer = null } }
    }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        when (name) {
            "textDetectorAvailability" -> reply(availability(args.opt("languages")))
            "detectText" -> still(args, reply)
            else -> reply(JSONObject().put("error", "NotSupportedError"))
        }
    }

    companion object {
        /** On main, before a client is made; under the agent no hatch is called (D13). */
        var optionsHatch: ((TextRecognizerOptions.Builder) -> Unit)? = null

        // Latin entries in ML Kit's supported, experimental and mapped language tables; regional
        // aliases map to their base language. No language model is selected by a requested tag.
        // https://developers.google.com/ml-kit/vision/text-recognition/v2/languages
        val supported = ("af sq ca hr cs da nl en et fil tl fi fr de hu is id it lv lt ms no pl pt ro sr-Latn sk sl es sv tr vi " +
            "az eu bs ceb eo gl ht ga jv la mt sw uz cy zu " +
            "ace ach ada ak alg arn ast ath ay ban bm bnt btk bem bik bi br cho cr mus dak dua efi ee fo fj fon ff gaa lg gay gil " +
            "got gn ha haw hz hil iba ig ilo kab kac kl kam kr kha ki rw kg kos kj ln nds loz lu luo mad mg man gv mi mh men " +
            "enm gmh min moh lol nah nv ng niu nd nso ny nyn tog nzi oc oj ang fro goh non pro pam pag pap qu rm rom rn sm sg " +
            "sco gd sn son st su ss ty tem to ts tn tk ve fy wo xh yo zap").split(" ")

        internal fun match(tag: String): String? {
            val parts = tag.replace('_', '-').split('-').toMutableList()
            // Truncation must not turn an explicitly non-Latin request into a Latin one.
            // RFC 5646 §2.1: up to three extlangs may precede the script after a 2–3-letter language.
            // https://www.rfc-editor.org/rfc/rfc5646.html#section-2.1
            fun letters(part: String, length: Int) = part.length == length && part.all { it in 'A'..'Z' || it in 'a'..'z' }
            val extlangs = if (parts.first().length in 2..3) parts.drop(1).take(3).takeWhile { letters(it, 3) }.size else 0
            val script = parts.getOrNull(1 + extlangs)?.takeIf { letters(it, 4) }
            if (script != null && !script.equals("Latn", true)) return null
            while (parts.isNotEmpty()) {
                val candidate = parts.joinToString("-").lowercase(Locale.ROOT)
                supported.firstOrNull { it.lowercase(Locale.ROOT) == candidate }
                    ?.let { return it }
                parts.removeAt(parts.lastIndex)
            }
            return null
        }

        internal fun languages(value: Any?): Pair<List<String>, String?> {
            if (value == null || value == JSONObject.NULL) return supported to null
            val asked = value as? JSONArray ?: return emptyList<String>() to value.toString()
            val matched = ArrayList<String>()
            for (i in 0 until asked.length()) {
                val tag = asked.optString(i)
                val hit = match(tag) ?: return emptyList<String>() to tag
                if (hit !in matched) matched += hit
            }
            return matched to null
        }

        internal fun availability(value: Any?): JSONObject {
            val (matched, unmatched) = languages(value)
            val answer = JSONObject().put("availability", if (unmatched == null) "available" else "unavailable")
                .put("languages", JSONArray(matched)).put("android", JSONObject().put("script", "Latn"))
            if (unmatched != null) answer.put("reason", "unsupported-options")
            return answer
        }

        // ML Kit already gives pixel boxes and clockwise corners from top left. Keep its block/line
        // order and BCP 47 language, including `und` when unknown; do not sort a multi-column page by y.
        // https://developers.google.com/ml-kit/vision/text-recognition/v2/android#4.-extract-text-from-blocks-of-recognized-text
        // https://developers.google.com/android/reference/com/google/mlkit/vision/text/Text.Line
        internal fun shape(line: Text.Line): JSONObject {
            val box = line.boundingBox
            return JSONObject().put("rawValue", line.text)
                .put("boundingBox", JSONObject().put("x", box?.left ?: 0).put("y", box?.top ?: 0)
                    .put("width", box?.width() ?: 0).put("height", box?.height() ?: 0))
                .put("cornerPoints", JSONArray((line.cornerPoints ?: emptyArray()).map { JSONObject().put("x", it.x).put("y", it.y) }))
                .put("android", JSONObject().put("recognizedLanguage", line.recognizedLanguage))
        }
    }

    private fun still(args: JSONObject, reply: (JSONObject) -> Unit) {
        val (_, unmatched) = languages(args.opt("languages"))
        if (unmatched != null) return reply(JSONObject().put("error", "NotSupportedError").put("message", "ML Kit's Latin recognizer reads no $unmatched"))
        val path = args.optString("image")
        val file = context.fileToRead(path) ?: return reply(JSONObject().put("error", "NotFoundError"))
        val options = TextRecognizerOptions.Builder()
        try {
            if (!context.agent) optionsHatch?.invoke(options)
        } catch (e: Exception) {
            return reply(JSONObject().put("error", "OperationError").put("message", e.message))
        }
        if (!watchingMemory) {
            context.application.registerComponentCallbacks(memory)
            watchingMemory = true
        }
        work.execute {
            val answer = try {
                val image = upright(file.path)
                if (image == null) JSONObject().put("error", "InvalidStateError").put("message", "decode")
                else try {
                    val client = recognizer ?: TextRecognition.getClient(options.build()).also { recognizer = it }
                    val found = Tasks.await(client.process(InputImage.fromBitmap(image, 0)))
                    JSONObject().put("texts", JSONArray(found.textBlocks.flatMap { it.lines }.map(::shape)))
                        .put("width", image.width).put("height", image.height)
                } finally { image.recycle() }
            } catch (e: Throwable) {
                val cause = if (e is ExecutionException) e.cause ?: e else e
                JSONObject().put("error", "OperationError").put("message", cause.message ?: "ML Kit failed")
            }
            main.post {
                answer.optJSONArray("texts")?.let { context.log("text-detection: $path: ${it.length()} lines") }
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

    override fun destroy(done: () -> Unit) {
        if (watchingMemory) context.application.unregisterComponentCallbacks(memory)
        work.execute { recognizer?.close(); recognizer = null; main.post { done() } }
        work.shutdown()
    }
}
