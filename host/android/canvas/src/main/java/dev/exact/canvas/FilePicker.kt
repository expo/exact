package dev.exact.canvas

import android.app.Activity
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.ColorSpace
import android.os.ext.SdkExtensions
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.InputStream

/** A file input imports copies under the result's temporary read grant (LLP 1069.002 D4–D8). */
internal object FilePicker {
    const val FALLBACK = "file picker: photo picker unavailable; using document picker"

    private var imports: PickerImports? = null

    fun start(context: Context): PickerImports = imports ?: PickerImports(context.cacheDir).also { imports = it }

    fun show(context: Context, accept: List<String>, multiple: Boolean, answer: (String?) -> Unit) {
        val app = context.applicationContext
        val main = Handler(Looper.getMainLooper())
        val photo = Build.VERSION.SDK_INT >= 33 ||
            (Build.VERSION.SDK_INT >= 30 && SdkExtensions.getExtensionVersion(Build.VERSION_CODES.R) >= 2)
        fun launch(usePhoto: Boolean) {
            val request = intent(accept, multiple, usePhoto)
            Extensions.start(request, failed = { reason ->
                if (request.action == MediaStore.ACTION_PICK_IMAGES && reason == "NotFoundError") {
                    answer(FALLBACK)
                    launch(false)
                } else answer("file picker: $reason")
            }) { code, data ->
                val uris = chosen(code, data, multiple)
                if (uris.isEmpty()) answer(null)
                else try {
                    val store = start(app)
                    // Result access lasts while the receiving activity's task stack is active.
                    // https://developer.android.com/training/secure-file-sharing/request-file
                    Thread({
                        val result = try {
                            copy(open(app.contentResolver, uris), accept, store::stage)
                        } catch (e: Exception) { failure(e) }
                        main.post { answer(result) }
                    }, "exact-file-picker").start()
                } catch (e: Exception) { answer(failure(e)) }
            }
        }
        try { launch(photo) } catch (e: RuntimeException) { answer(failure(e)) }
    }

    private fun convertsHeic(accept: List<String>): Boolean {
        val types = mimeTypes(accept)
        return "image/jpeg" in types &&
            accept.none { it.trim().lowercase(java.util.Locale.ROOT) in listOf("*/*", "image/*", "image/heic", "image/heif", ".heic", ".heif") }
    }

    fun mimeTypes(accept: List<String>): List<String> = accept.map { raw ->
        val type = raw.trim().lowercase(java.util.Locale.ROOT)
        if (type.startsWith('.')) MimeTypeMap.getSingleton().getMimeTypeFromExtension(type.drop(1)) ?: "*/*"
        else type
    }.filter { it.isNotEmpty() }.distinct()

    fun intent(accept: List<String>, multiple: Boolean, photoAvailable: Boolean): Intent {
        val types = mimeTypes(accept)
        val visual = types.isNotEmpty() && types.all { it == "image/*" || it == "video/*" }
        if (photoAvailable && visual) return Intent(MediaStore.ACTION_PICK_IMAGES).apply {
            type = types.singleOrNull()
            if (multiple) putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX,
                if (Build.VERSION.SDK_INT >= 33) MediaStore.getPickImagesMaxLimit() else 100)
        }
        val offered = if (convertsHeic(accept)) (types + listOf("image/heic", "image/heif")).distinct() else types
        return Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).apply {
            type = if ("*/*" in offered) "*/*" else offered.singleOrNull() ?: "*/*"
            if (offered.size > 1 && "*/*" !in offered) putExtra(Intent.EXTRA_MIME_TYPES, offered.toTypedArray())
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple)
        }
    }

    fun chosen(code: Int, data: Intent?, multiple: Boolean): List<Uri> {
        if (code != Activity.RESULT_OK || data == null) return emptyList()
        val clip = data.clipData
        val uris = if (clip == null) listOfNotNull(data.data)
            else (0 until clip.itemCount).map { clip.getItemAt(it).uri }
        return uris.distinct().let { if (multiple) it else it.take(1) }
    }

    private data class Source(val name: String, val mime: String?, val input: InputStream)

    private fun open(resolver: ContentResolver, uris: List<Uri>): List<Source> {
        val opened = ArrayList<Source>()
        try {
            for ((index, uri) in uris.withIndex()) {
                val name = displayName(resolver, uri, index)
                val mime = resolver.getType(uri)?.takeIf { it.isNotBlank() }
                    ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT))
                val input = resolver.openInputStream(uri) ?: throw IOException("no stream for the chosen file")
                opened.add(Source(name, mime, input))
            }
            return opened
        } catch (e: Exception) {
            opened.forEach { runCatching { it.input.close() } }
            throw e
        }
    }

    fun copy(resolver: ContentResolver, cache: File, uris: List<Uri>): String = try {
        copy(open(resolver, uris), emptyList()) { java.nio.file.Files.createTempDirectory(cache.toPath(), "exact-picked-").toFile() }
    } catch (e: Exception) { failure(e) }

    /** Each copy has its own directory so equal display names cannot overwrite one another. */
    private fun copy(sources: List<Source>, accept: List<String>, stage: () -> File): String {
        val directories = ArrayList<File>()
        val picked = JSONArray()
        return try {
            for (source in sources) {
                val dir = stage()
                directories.add(dir)
                val name = source.name
                var mime = source.mime
                var file = File.createTempFile("source-", null, dir)
                source.input.use { from -> Files.output(file).use { to -> from.copyTo(to) } }
                val converted = mime in listOf("image/heic", "image/heif") && convertsHeic(accept)
                if (converted) {
                    val jpeg = File.createTempFile("converted-", ".jpg", dir)
                    transcode(file, jpeg)
                    file.delete()
                    file = jpeg
                    mime = "image/jpeg"
                }
                val extensions = JSONObject()
                for (token in accept.map { it.trim().lowercase(java.util.Locale.ROOT) }.filter { it.startsWith('.') }) {
                    MimeTypeMap.getSingleton().getMimeTypeFromExtension(token.drop(1))?.let { extensions.put(token, it) }
                }
                picked.put(JSONObject().put("path", file.absolutePath).put("name", name)
                    .put("type", mime ?: JSONObject.NULL).put("extensions", extensions).put("converted", converted)
                    .put("name_type", MimeTypeMap.getSingleton().getMimeTypeFromExtension(
                        name.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)) ?: JSONObject.NULL)
                    .put("extension", mime?.let { MimeTypeMap.getSingleton().getExtensionFromMimeType(it) } ?: JSONObject.NULL))
            }
            picked.toString()
        } catch (e: Exception) {
            directories.forEach { it.deleteRecursively() }
            failure(e)
        } catch (_: OutOfMemoryError) {
            directories.forEach { it.deleteRecursively() }
            "file picker: image decode did not fit in memory"
        } finally {
            sources.forEach { runCatching { it.input.close() } }
        }
    }

    private fun transcode(source: File, destination: File) {
        // ImageDecoder applies orientation. Encoding the upright pixels writes none of the provider's EXIF/GPS.
        val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(source)) { decoder, _, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetColorSpace(ColorSpace.get(ColorSpace.Named.SRGB))
        }
        try {
            Files.output(destination).use {
                if (!bitmap.compress(Bitmap.CompressFormat.JPEG, 90, it)) throw IOException("JPEG encode failed")
            }
        } finally { bitmap.recycle() }
    }

    private fun failure(e: Exception) = "file picker: ${e.message ?: e.javaClass.simpleName}"

    private fun displayName(resolver: ContentResolver, uri: Uri, index: Int): String {
        val supplied = try {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                if (it.moveToFirst()) it.getString(0) else null
            }
        } catch (_: Exception) { null }
        return supplied?.substringAfterLast('/')?.substringAfterLast('\\')?.replace("\u0000", "")
            ?.takeIf { it.isNotBlank() && it != "." && it != ".." } ?: "file-$index"
    }
}

/** Only picker staging lives here; a new process removes copies its predecessor could not deliver. */
internal class PickerImports(cache: File) {
    private val root = File(cache, "exact-picker")

    init {
        if (root.exists() && !root.deleteRecursively()) throw IOException("could not clean picker staging")
        if (!root.mkdirs()) throw IOException("could not create picker staging")
    }

    fun stage(): File {
        java.nio.file.Files.createDirectories(root.toPath())
        return java.nio.file.Files.createTempDirectory(root.toPath(), "exact-picked-").toFile()
    }
}
