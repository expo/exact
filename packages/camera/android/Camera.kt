// @exact/camera on Android (LLP 1133.008 D1-D5, D8, D10): the viewfinder `exact-camera`, CameraX's `PreviewView`
// bound as a `Preview` use case, over a video track of @exact/media-capture's, and `takePhoto` through the capture's
// one `ImageCapture`, written to the app's files.
//
// - A viewfinder is a consumer of its track while it is displayed and its `srcObject` names the track (D3), joined
//   with the origin the runner reports, so a tool's display takes no lease (§14). The process has one `Preview`: it
//   shows in the viewfinder displayed last, and another displayed at once shows its empty box (D10).
// - The `ImageCapture` is the capture's process object (D3): each session's photos in turn. Unsaid, it is CameraX's
//   own (`CAPTURE_MODE_MINIMIZE_LATENCY`, flash off, JPEG, D2); `fillLightMode` and the `android.*` words set the
//   flash, mode and quality, a builder field built again when it changes. The shutter sound is
//   `MediaActionSound.SHUTTER_CLICK` at each capture, unless `android.shutterSound` is false where the platform
//   allows it. Before API 33 the regional requirement cannot be queried, so the sound stays on.
// - Hosted views take no touches beyond a click (`host/android/README.md`): no tap to focus or pinch to zoom yet (D8
//   item 6, recorded as a tell).
// - Under the agent no capture exists: the viewfinder shows a drawn test card, mirrored for a `user` camera, and
//   `takePhoto` writes it as JPEG, or the app asset `prefer extension camera.picture={"path":…}` names, unchanged.
//   The drawn card is Android's own (the package's card.jpg is not in the APK), so its bytes differ from the other
//   hosts'.
// - Tier 3 (D5): `useCasesHatch` sees the use cases before each bind, `imageCaptureHatch` each `ImageCapture.Builder`
//   after the module's words, `cameraHatch` the `Camera` after each bind.
// - `takePhotoWithCameraApp` is the system's camera app (`MediaStore.ACTION_IMAGE_CAPTURE`, what Jetpack's
//   `ActivityResultContracts.TakePicture` sends), with its own shutter, flash and review. It writes a file staged
//   outside the app's directories through the reader's `Files` provider, under a write grant for that file while it is
//   up; the photo replaces the answered path only when the person keeps it, so a cancel leaves what was there, and
//   nothing reaches MediaStore or the gallery. An app that uses only this asks for no permission. Android refuses the
//   intent to an app that declares CAMERA (`getUserMedia` derives it) without holding it, so such an
//   app asks for it first, as a hand-built one must. Under the agent the runner holds the person's sheet, and `take`
//   writes the agent's picture.
// - `recordVideoWithCameraApp` is the same camera app recording a video (`MediaStore.ACTION_VIDEO_CAPTURE`, Jetpack's
//   `CaptureVideo`), staged and kept the same way and answered as a `Video` read with `MediaMetadataRetriever` (LLP
//   1133.008.000 D2). `maxDuration` is `EXTRA_DURATION_LIMIT`, `android.videoQuality` and `android.sizeLimit` its other
//   extras. A camera app that ignores `EXTRA_OUTPUT` and answers a video URI of its own has it copied in; its gallery
//   copy is its own. The camera app records the sound under its own microphone permission. Under the agent `take`
//   writes the package's one-second clip, the same bytes on every host.
// - A call let go publishes nothing (LLP 1133.008.000 D7): the camera app stays up, as another app's UI must, and
//   what the person keeps there is removed. The kept file replaces the path on the main thread, where `cancel` runs,
//   after a copy-in and the facts are read off it.
package exact.camera

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.media.MediaActionSound
import android.os.Build
import android.provider.MediaStore
import android.view.View
import android.widget.ImageView
import androidx.camera.core.Camera
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.Preview
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Files
import dev.exact.canvas.TrackError
import dev.exact.canvas.TrackConsumer
import dev.exact.canvas.VideoSource
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.nio.file.StandardCopyOption

class CameraModule(context: ExtensionContext) : ExactExtension(context) {
    companion object {
        @JvmStatic var useCasesHatch: ((MutableList<Any>) -> Unit)? = null
        @JvmStatic var imageCaptureHatch: ((ImageCapture.Builder) -> Unit)? = null
        @JvmStatic var cameraHatch: ((Camera) -> Unit)? = null
        /** The one `Preview` of the process, and the viewfinder that shows it. */
        private var preview: Preview? = null
        private var showing: PreviewView? = null
    }

    /** What the runner said of a viewfinder, and what it consumes. */
    private class ViewState {
        var view: View? = null
        var src: String? = null
        var changes = 0
        var reports = 0
        var shown = false
        var madeBy = "app"
        var sub: Sub? = null
        var picture: Bitmap? = null
    }

    private class Sub(val track: String, val origin: String) : TrackConsumer {
        var live = false
        private var source: VideoSource? = null
        var leave: () -> Unit = {}
        var onLive: (VideoSource?) -> Unit = {}
        var onBlank: () -> Unit = {}
        override fun liveVideo(source: VideoSource?) {
            live = true
            this.source = source
            onLive(source)
        }
        override fun muted(reason: String) = onBlank()
        override fun unmuted() { if (live) onLive(source) }
        override fun ended() {
            live = false
            source = null
            onBlank()
        }
    }

    private val views = HashMap<String, ViewState>()
    private var picture: String? = null
    private var taken = 0
    private val photos = JSONArray()
    private val videos = JSONArray()
    private val sound by lazy { MediaActionSound().apply { load(MediaActionSound.SHUTTER_CLICK) } }

    private fun say(line: String) = context.log("camera: $line")
    private fun error(code: String) = JSONObject().put("error", code)

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        when (name) {
            "takePhoto" -> takePhoto(args, reply)
            "getPhotoCapabilities" -> reply(capabilities(args))
            "picture" -> reply(JSONObject().put("path", picture ?: ""))
            else -> reply(error("NotSupportedError"))
        }
    }

    override fun later(name: String, args: JSONObject, preferred: Any?, reply: (JSONObject) -> Unit) {
        val tap = (preferred as? JSONObject)?.optString("tap")
        when (name) {
            "takePhotoWithCameraApp" -> cameraApp(PHOTO, args, tap, reply)
            "recordVideoWithCameraApp" -> cameraApp(VIDEO, args, tap, reply)
            else -> later(name, args, reply)
        }
    }

    /** The send [request] was let go (LLP 1133.016 D15 item 2): its camera app's result publishes nothing. */
    override fun cancel(request: Int) {
        if (cameraAppUp && request == cameraAppRequest) {
            cameraAppLetGo = true
            say("camera app: let go")
        }
    }

    override fun preferred(values: JSONObject) {
        val next = values.optJSONObject("picture")?.optString("path")?.ifEmpty { null }
        if (next == picture) return
        picture = next
        for (v in views.values) if (v.picture != null) v.sub?.let { show(v, it, null) }
    }

    // Viewfinders (D3, D8 items 1 and 2)

    override fun view(tag: String, session: String, id: String, context: Context, props: JSONObject, emit: (String, String) -> Unit): View {
        require(tag == "exact-camera") { "no view $tag" }
        val v: View = if (this.context.agent) ImageView(context).apply { setBackgroundColor(Color.BLACK) }
            else PreviewView(context).apply { implementationMode = PreviewView.ImplementationMode.COMPATIBLE }
        state(id).view = v
        update(v, props)
        return v
    }

    override fun update(view: View, props: JSONObject) {
        val id = views.entries.firstOrNull { it.value.view === view }?.key ?: return
        // `contain` unsaid is `<video>`'s; PreviewView's own default (FILL_CENTER) is a written `cover` (D2).
        when (view) {
            is PreviewView -> view.scaleType = when (props.optString("object-fit")) {
                "cover" -> PreviewView.ScaleType.FILL_CENTER
                else -> PreviewView.ScaleType.FIT_CENTER
            }
            is ImageView -> view.scaleType = when (props.optString("object-fit")) {
                "cover" -> ImageView.ScaleType.CENTER_CROP
                "fill" -> ImageView.ScaleType.FIT_XY
                else -> ImageView.ScaleType.FIT_CENTER
            }
        }
        val v = state(id)
        if (!props.has("srcObject")) v.src = ""
        else props.optString("srcObject").let { if (it != v.src) { v.src = it; v.changes += 1 } }
        reconcile(id)
    }

    override fun destroyView(view: View) {
        val v = views.values.firstOrNull { it.view === view } ?: return
        // The reader lets a hidden viewfinder's view go (`display: none`) and makes another when it shows again,
        // under the same id: what the runner said of it stands, since it reports a `srcObject` it bound only once.
        blank(v)
        v.view = null
        v.sub?.leave?.invoke()
        v.sub = null
    }

    override fun displayed(session: String, id: String, shown: Boolean, origin: String) {
        val v = state(id)
        v.shown = shown
        if (shown) v.madeBy = origin
        reconcile(id)
    }

    override fun bound(session: String, id: String, prop: String, origin: String) {
        if (prop != "srcObject") return
        val v = state(id)
        v.reports += 1
        v.madeBy = origin
        reconcile(id)
    }

    private fun state(id: String) = views.getOrPut(id) { ViewState() }

    private fun reconcile(id: String) {
        val v = views[id] ?: return
        val src = v.src ?: ""
        val want = v.view != null && v.shown && src.isNotEmpty() && v.reports >= v.changes && context.tracks.lookup(src)?.kind == "video"
        v.sub?.let { if (!want || it.track != src) { blank(v); it.leave(); v.sub = null } }
        v.sub?.let { if (want && !it.live && v.madeBy == "app" && it.origin != "app") { it.leave(); v.sub = null } }
        if (!want || v.sub != null) return
        val sub = Sub(src, v.madeBy)
        sub.onLive = { source -> show(v, sub, source) }
        sub.onBlank = { if (v.sub === sub) blank(v) }
        v.sub = sub
        try {
            sub.leave = context.tracks.subscribe(src, sub, false, v.madeBy)
            say("viewfinder $id consumes $src (${v.madeBy} origin)")
        } catch (e: Exception) {
            v.sub = null
            say("viewfinder $id: ${e.message}")
        }
    }

    private fun show(v: ViewState, sub: Sub, source: VideoSource?) {
        if (v.sub !== sub) return
        val facing = context.tracks.lookup(sub.track)?.state?.invoke()?.optJSONObject("settings")?.optString("facingMode")
        when (val view = v.view) {
            is ImageView -> {
                val bitmap = agentBitmap()
                view.setImageBitmap(bitmap)
                v.picture = bitmap
                context.tracks.lookup(sub.track)?.picture = bitmap
                view.scaleX = if (facing == "user") -1f else 1f
            }
            is PreviewView -> {
                val p = preview ?: Preview.Builder().build().also { preview = it }
                p.surfaceProvider = view.surfaceProvider
                showing = view
                val cases = mutableListOf<Any>(p)
                useCasesHatch?.invoke(cases)
                cases.forEach(source!!::add)
                (source.camera as? Camera)?.let { cameraHatch?.invoke(it) }
            }
        }
    }

    private fun blank(v: ViewState) {
        v.sub?.let { sub ->
            context.tracks.lookup(sub.track)?.let { entry ->
                if (v.picture != null && entry.picture === v.picture) {
                    entry.picture = views.values.firstOrNull { it !== v && it.sub?.track == sub.track && it.picture != null }?.picture
                }
            }
        }
        v.picture = null
        when (val view = v.view) {
            is ImageView -> view.setImageDrawable(null)
            is PreviewView -> if (showing === view) {
                showing = null
                preview?.surfaceProvider = null
            }
        }
    }

    // Photos (D1, D2)

    /** Under the agent, each photo for `state.extensions` (§9, Disclosure), by path and SHA-256; a photo from the camera
     * app names no track. */
    private fun disclose(path: String, track: String?, bytes: ByteArray, into: JSONArray = photos) {
        val sha = java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
        val entry = JSONObject().put("path", path).put("sha256", sha).put("origin", context.origin)
        if (into === photos) entry.put("track", track ?: JSONObject.NULL)
        into.put(entry)
        context.state(JSONObject().put("photos", photos).put("videos", videos))
    }

    private fun takePhoto(args: JSONObject, reply: (JSONObject) -> Unit) {
        val entry = context.tracks.lookup(args.optString("track")) ?: return reply(error("NotFoundError"))
        if (entry.kind != "video") return reply(error("NotSupportedError"))
        val s = entry.state?.invoke() ?: JSONObject()
        // A track that is not shown takes no photo (D1): ended, disabled, or muted (`idle` among them).
        if (s.optString("readyState") != "live" || !s.optBoolean("enabled") || s.optBoolean("muted", true)) return reply(error("InvalidStateError"))
        val ext = "jpg"
        val path = args.optString("path").ifEmpty { null }?.let { if (it.endsWith(".$ext")) it else "$it.$ext" } ?: "app:/tmp/camera/${++taken}.$ext"
        val file = context.file(path) ?: return reply(error("UnknownError"))
        file.parentFile?.mkdirs()
        if (context.agent) {
            val bytes = agentBytes() ?: return reply(error("UnknownError"))
            try { dev.exact.canvas.Files.write(file, bytes) } catch (e: java.io.IOException) { return reply(error("UnknownError")) }
            say("took $path (${bytes.size} bytes, the agent's picture)")
            disclose(path, entry.id, bytes)
            return reply(photo(path, file))
        }
        val source = entry.video?.invoke() ?: return reply(error("InvalidStateError"))
        val android = args.optJSONObject("android") ?: JSONObject()
        val capture = try { imageCapture(source, args, android) } catch (e: TrackError) { return reply(error(e.code)) }
        if (android.optBoolean("shutterSound", true) || Build.VERSION.SDK_INT < 33 || MediaActionSound.mustPlayShutterSound()) sound.play(MediaActionSound.SHUTTER_CLICK)
        // A photo in flight is a consumer: its lease holds until the file is written (D3).
        val holder = Sub(entry.id, "app")
        holder.leave = context.tracks.subscribe(entry.id, holder, false, "app")
        capture.takePicture(ImageCapture.OutputFileOptions.Builder(file).build(), ContextCompat.getMainExecutor(context.application),
            object : ImageCapture.OnImageSavedCallback {
                override fun onImageSaved(output: ImageCapture.OutputFileResults) {
                    holder.leave()
                    say("took $path (${file.length()} bytes)")
                    reply(photo(path, file))
                }
                override fun onError(e: ImageCaptureException) {
                    holder.leave()
                    say("takePhoto: ${e.message}")
                    reply(error("UnknownError"))
                }
            })
    }

    // The camera app (`takePhotoWithCameraApp`, `recordVideoWithCameraApp`)

    /** What the camera app is asked for: its intent, the staged file's name and extension, and its noun. */
    private class Shot(val action: String, val ext: String, val noun: String)
    private val PHOTO = Shot(MediaStore.ACTION_IMAGE_CAPTURE, "jpg", "photo")
    private val VIDEO = Shot(MediaStore.ACTION_VIDEO_CAPTURE, "mp4", "video")

    /** Whether the camera app is up for a call; one is at a time, a photo's or a video's. */
    private var cameraAppUp = false
    /** The runner's number for that call, which a `cancel` names, and whether it was let go. */
    private var cameraAppRequest: Int? = null
    private var cameraAppLetGo = false

    /** The answer of `takePhotoWithCameraApp` or `recordVideoWithCameraApp`, [tap] the agent's on its sheet. */
    private fun cameraApp(shot: Shot, args: JSONObject, tap: String?, reply: (JSONObject) -> Unit) {
        if (cameraAppUp) return reply(error("InvalidStateError"))
        val ext = shot.ext
        val path = args.optString("path").ifEmpty { null }?.let { if (it.endsWith(".$ext")) it else "$it.$ext" } ?: "app:/tmp/camera/${++taken}.$ext"
        val file = context.file(path) ?: return reply(error("UnknownError"))
        if (context.agent) {
            if (tap != "take") {
                say("camera app: cancelled (the agent's sheet)")
                return reply(error("AbortError"))
            }
            val bytes = (if (shot === VIDEO) agentVideo() else agentBytes()) ?: return reply(error("UnknownError"))
            file.parentFile?.mkdirs()
            try { Files.write(file, bytes) } catch (e: IOException) { return reply(error("UnknownError")) }
            say("camera app took $path (${bytes.size} bytes, the agent's ${shot.noun})")
            disclose(path, null, bytes, if (shot === VIDEO) videos else photos)
            return reply(if (shot === VIDEO) video(path, file) else photo(path, file))
        }
        val request = context.request
        val camera = android.Manifest.permission.CAMERA
        if (!declares(camera) || held(camera)) return openCameraApp(shot, request, path, file, args, reply)
        cameraAppUp = true
        cameraAppRequest = request
        cameraAppLetGo = false
        context.askPermission(camera) {
            cameraAppUp = false
            if (cameraAppLetGo) return@askPermission reply(error("AbortError"))
            if (held(camera)) return@askPermission openCameraApp(shot, request, path, file, args, reply)
            say("camera app: the app declares CAMERA and does not hold it")
            reply(error("NotAllowedError"))
        }
    }

    private fun held(permission: String) = context.application.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    /** Whether the app's manifest asks for [permission]. */
    private fun declares(permission: String): Boolean = runCatching {
        val app = context.application
        @Suppress("DEPRECATION")
        app.packageManager.getPackageInfo(app.packageName, PackageManager.GET_PERMISSIONS).requestedPermissions?.contains(permission) == true
    }.getOrDefault(false)

    private fun openCameraApp(shot: Shot, request: Int?, path: String, file: File, args: JSONObject, reply: (JSONObject) -> Unit) {
        // Outside `app:/cache` and `app:/tmp` (`cache/cache`, `cache/temporary`), where the app's own code writes. A
        // file staged before the process last ended answers nothing now.
        val staging = File(context.application.cacheDir, "exact.camera-app")
        Files.removeTree(staging)
        val staged = File(staging, "${shot.noun}.${shot.ext}")
        val out = Files.writableUri(context.application, staged) ?: return reply(error("UnknownError"))
        val intent = Intent(shot.action).putExtra(MediaStore.EXTRA_OUTPUT, out)
            .addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        // Android names no extra for the side; these are the ones camera apps read (Google Camera among them).
        when (args.optString("facingMode")) {
            "user" -> intent.putExtra("android.intent.extras.CAMERA_FACING", 1).putExtra("android.intent.extras.LENS_FACING_FRONT", 1)
                .putExtra("android.intent.extra.USE_FRONT_CAMERA", true)
            "environment" -> intent.putExtra("android.intent.extras.CAMERA_FACING", 0).putExtra("android.intent.extras.LENS_FACING_BACK", 1)
        }
        if (shot === VIDEO) {
            args.optDouble("maxDuration").takeIf { it > 0 }?.let { intent.putExtra(MediaStore.EXTRA_DURATION_LIMIT, kotlin.math.ceil(it).toInt()) }
            val android = args.optJSONObject("android") ?: JSONObject()
            when (android.optString("videoQuality")) {
                "low" -> intent.putExtra(MediaStore.EXTRA_VIDEO_QUALITY, 0)
                "high" -> intent.putExtra(MediaStore.EXTRA_VIDEO_QUALITY, 1)
            }
            android.optDouble("sizeLimit").takeIf { it > 0 }?.let { intent.putExtra(MediaStore.EXTRA_SIZE_LIMIT, it.toLong()) }
        }
        cameraAppUp = true
        cameraAppRequest = request
        cameraAppLetGo = false
        say("camera app for $path")
        val end = { reason: String ->
            Files.endWrite(staged)
            Files.removeTree(staging)
            cameraAppUp = false
            cameraAppRequest = null
            say("camera app: $reason")
        }
        context.startActivityForResult(intent, { refusal ->
            end("not started, $refusal")
            reply(error(refusal))
        }) { code, data ->
            Files.endWrite(staged)
            if (code != Activity.RESULT_OK) {
                end("cancelled")
                return@startActivityForResult reply(error("AbortError"))
            }
            // A camera app that records to its own place despite `EXTRA_OUTPUT` answers its URI; copied in, and the
            // facts read, off the main thread. The kept file is published back on it, where `cancel` runs.
            val own = if (shot === VIDEO && !(Files.regular(staged) && staged.length() > 0)) data?.data else null
            Thread {
                val copied = own == null || copyIn(own, staged)
                val facts = if (copied && Files.regular(staged) && staged.length() > 0) {
                    if (shot === VIDEO) video(path, staged) else photo(path, staged)
                } else null
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    when {
                        cameraAppLetGo -> { end("let go; kept nothing at $path"); reply(error("AbortError")) }
                        facts == null -> { end("kept nothing at $path"); reply(error("UnknownError")) }
                        runCatching { keep(staged, file) }.isFailure -> { end("could not keep $path"); reply(error("UnknownError")) }
                        else -> { end("took $path (${file.length()} bytes)"); reply(facts) }
                    }
                }
            }.apply { name = "exact-camera-app" }.start()
        }
    }

    /** The camera app's own video at [uri], copied to [staged] 64 KiB at a time: a `video/` type only, never one of the
     * app's own files. A copy that fails leaves no staged file. */
    private fun copyIn(uri: android.net.Uri, staged: File): Boolean {
        val resolver = context.application.contentResolver
        if (uri.scheme != "content" || uri.authority == "${context.application.packageName}.exact.files") {
            say("camera app: answered $uri, not read")
            return false
        }
        val type = runCatching { resolver.getType(uri) }.getOrNull()
        if (type?.startsWith("video/") != true) {
            say("camera app: answered $uri of type $type, not read")
            return false
        }
        return runCatching {
            val input = resolver.openInputStream(uri) ?: throw IOException("no stream")
            input.use { Files.output(staged).use { out -> input.copyTo(out, 64 shl 10) } }
            say("camera app answered its own $uri; copied, and its gallery copy is the camera app's")
        }.onFailure {
            staged.delete()
            say("camera app: could not read $uri, ${it.message}")
        }.isSuccess
    }

    /** [staged] in place of [file]: renamed over it, which replaces a link there rather than writing what it names. */
    private fun keep(staged: File, file: File) {
        file.parentFile?.mkdirs()
        try {
            java.nio.file.Files.move(staged.toPath(), file.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE)
        } catch (e: java.nio.file.AtomicMoveNotSupportedException) {
            Files.output(file).use { out -> staged.inputStream().use { it.copyTo(out) } }
        }
    }

    /** The capture's `ImageCapture`, built again when a builder word changes (D4). */
    private fun imageCapture(source: VideoSource, args: JSONObject, android: JSONObject): ImageCapture {
        val mode = when (android.optString("captureMode")) {
            "maximizeQuality" -> ImageCapture.CAPTURE_MODE_MAXIMIZE_QUALITY
            "zeroShutterLag" -> ImageCapture.CAPTURE_MODE_ZERO_SHUTTER_LAG
            else -> ImageCapture.CAPTURE_MODE_MINIMIZE_LATENCY
        }
        val quality = android.optInt("jpegQuality", 0)
        val key = "camera.imageCapture.$mode.$quality"
        val made = source.shared(key) {
            val builder = ImageCapture.Builder().setCaptureMode(mode)
            if (quality in 1..100) builder.setJpegQuality(quality)
            imageCaptureHatch?.invoke(builder)
            builder.build().also { if (!source.attach(it)) throw TrackError("UnknownError") }
        }
        val flash = android.optString("flashMode").ifEmpty { null } ?: when (args.optString("fillLightMode")) { "flash" -> "on"; "auto" -> "auto"; else -> "off" }
        made.flashMode = when (flash) { "on" -> ImageCapture.FLASH_MODE_ON; "auto" -> ImageCapture.FLASH_MODE_AUTO; else -> ImageCapture.FLASH_MODE_OFF }
        return made
    }

    /** `Video`: its type, length in seconds, and size as it plays, its rotation applied; 0 where the file says nothing. */
    private fun video(path: String, file: File): JSONObject {
        val o = JSONObject().put("path", path).put("type", "video/mp4").put("size", file.length())
            .put("duration", 0).put("width", 0).put("height", 0)
        val r = android.media.MediaMetadataRetriever()
        try {
            r.setDataSource(file.path)
            val n = { key: Int -> r.extractMetadata(key)?.toLongOrNull() ?: 0L }
            r.extractMetadata(android.media.MediaMetadataRetriever.METADATA_KEY_MIMETYPE)?.takeIf { it.startsWith("video/") }?.let { o.put("type", it) }
            val w = n(android.media.MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)
            val h = n(android.media.MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)
            val turned = n(android.media.MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION) % 180 == 90L
            o.put("duration", n(android.media.MediaMetadataRetriever.METADATA_KEY_DURATION) / 1000.0)
                .put("width", if (turned) h else w).put("height", if (turned) w else h)
        } catch (e: Exception) {
            say("video $path: ${e.message}")
        } finally {
            runCatching { r.release() }
        }
        return o
    }

    /** `Photo`: its size as it is shown, its orientation applied. */
    private fun photo(path: String, file: File): JSONObject {
        val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, o)
        val turned = runCatching { android.media.ExifInterface(file.path).getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, 1) >= 5 }.getOrDefault(false)
        return JSONObject().put("path", path).put("type", "image/jpeg").put("size", file.length())
            .put("width", if (turned) o.outHeight else o.outWidth).put("height", if (turned) o.outWidth else o.outHeight)
    }

    private fun capabilities(args: JSONObject): JSONObject {
        val entry = context.tracks.lookup(args.optString("track")) ?: return error("NotFoundError")
        if (entry.kind != "video") return error("NotSupportedError")
        val settings = entry.state?.invoke()?.optJSONObject("settings") ?: JSONObject()
        val torch = entry.state?.invoke()?.optJSONObject("capabilities")?.optBoolean("torch") == true
        val fixed = { n: Int -> JSONObject().put("min", n).put("max", n).put("step", 1) }
        return JSONObject().put("redEyeReduction", "never").put("imageWidth", fixed(settings.optInt("width"))).put("imageHeight", fixed(settings.optInt("height")))
            .put("fillLightMode", if (torch) JSONArray().put("auto").put("off").put("flash") else JSONArray().put("off"))
    }

    // The agent's picture and clip (§9)

    /** The package's one-second clip (`fixture/clip.mp4`), the same bytes on every host. */
    private fun agentVideo(): ByteArray? = runCatching { java.util.Base64.getDecoder().decode(AGENT_VIDEO) }.getOrNull()

    private fun agentBytes(): ByteArray? {
        picture?.let { p -> return runCatching { context.application.assets.open("app/$p").use { it.readBytes() } }.getOrNull() }
        val out = ByteArrayOutputStream()
        agentBitmap().compress(Bitmap.CompressFormat.JPEG, 85, out)
        return out.toByteArray()
    }

    /** A test card whose top and sides read at a glance: colour bars across the top, an F. */
    private fun agentBitmap(): Bitmap {
        picture?.let { p -> runCatching { context.application.assets.open("app/$p").use(BitmapFactory::decodeStream) }.getOrNull()?.let { return it } }
        val b = Bitmap.createBitmap(1280, 960, Bitmap.Config.ARGB_8888)
        val c = Canvas(b)
        c.drawColor(Color.rgb(0x20, 0x24, 0x2c))
        val bars = intArrayOf(0xffe53935.toInt(), 0xfffb8c00.toInt(), 0xfffdd835.toInt(), 0xff43a047.toInt(), 0xff1e88e5.toInt(), 0xff8e24aa.toInt(), Color.WHITE)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        bars.forEachIndexed { i, colour -> paint.color = colour; c.drawRect(i * 1280f / bars.size, 0f, (i + 1) * 1280f / bars.size, 160f, paint) }
        paint.color = Color.WHITE
        paint.textSize = 56f
        c.drawText("TOP LEFT", 40f, 240f, paint)
        paint.textAlign = Paint.Align.RIGHT
        c.drawText("TOP RIGHT", 1240f, 240f, paint)
        paint.textAlign = Paint.Align.CENTER
        paint.textSize = 260f
        c.drawText("F", 640f, 700f, paint)
        return b
    }
}

/** [CameraModule.agentVideo]'s bytes in Base64: `packages/camera/fixture/clip.mp4`, one second of colour bars, 160×120
 * H.264 in MP4 (`ffmpeg -f lavfi -i smptebars=size=160x120:rate=1:duration=1 -c:v libx264 -profile:v baseline -g 1
 * -crf 35 -bsf:v filter_units=remove_types=6 -movflags +faststart`). */
private val AGENT_VIDEO = listOf(
    "AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAALvbW9vdgAAAGxtdmhkAAAAAAAAAAAAAAAAAAAD6AAAA+gAAQAAAQAAAAAAAAAAAAAA",
    "AAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAj50cmFrAAAAXHRraGQAAAAD",
    "AAAAAAAAAAAAAAABAAAAAAAAA+gAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAKAAAAB4AAAAAAAk",
    "ZWR0cwAAABxlbHN0AAAAAAAAAAEAAAPoAAAAAAABAAAAAAG2bWRpYQAAACBtZGhkAAAAAAAAAAAAAAAAAABAAAAAQABVxAAAAAAALWhkbHIAAAAA",
    "AAAAAHZpZGUAAAAAAAAAAAAAAABWaWRlb0hhbmRsZXIAAAABYW1pbmYAAAAUdm1oZAAAAAEAAAAAAAAAAAAAACRkaW5mAAAAHGRyZWYAAAAAAAAA",
    "AQAAAAx1cmwgAAAAAQAAASFzdGJsAAAAvXN0c2QAAAAAAAAAAQAAAK1hdmMxAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAKAAeABIAAAASAAAAAAA",
    "AAABDExhdmMgbGlieDI2NAAAAAAAAAAAAAAAAAAAAAAAAAAAGP//AAAAM2F2Y0MBQsAK/+EAG2dCwArcKEflwFqBAQKgAAADACAAAAMAQeJE8AEA",
    "BWjOAlLIAAAAEHBhc3AAAAABAAAAAQAAABRidHJ0AAAAAAAADNAAAAAAAAAAGHN0dHMAAAAAAAAAAQAAAAEAAEAAAAAAHHN0c2MAAAAAAAAAAQAA",
    "AAEAAAABAAAAAQAAABRzdHN6AAAAAAAAAZoAAAABAAAAFHN0Y28AAAAAAAAAAQAAAx8AAAA9dWR0YQAAADVtZXRhAAAAAAAAACFoZGxyAAAAAAAA",
    "AABtZGlyYXBwbAAAAAAAAAAAAAAAAAhpbHN0AAAACGZyZWUAAAGibWRhdAAAAZZliIQEvJigACKhEQSA4X4gABgCxFREEtHxGKAAIXccAAR3I4AA",
    "jOxEEgOF+IAAYAsRURBKKNHxGKAAIU8cAAR3I4AAkuREEgOF+IAAYAsRURBKKNHxGKAAIWscAAR3I4AAjO109PT09PT09PT109PT09PT09PT109P",
    "T09PT09PT109PT09PT09PT8f/+GwRCgACAH5s8QACACISBrBkpZSUvJpN/h8Agg4Z+0AN8J3StXjCC4zwZvgXTsXLALojX/xp6aev8+n3EPgGHQd",
    "g2mX5cwMGhhACDLBkpZAp5YMlLGS6P+AYhocdeKAD584RGQoQXEb4HW5fWB0xy/+Aa9AuPpwkkYCPgEdckD4Y1KAfiGAaDYoUPiQkeIRIQABAq+N",
    "PDrOWHWcv4BgHDQLIoPgTMIFhVdvwPRSxaCoVA4EZfaa408Q+IYBoJhYUAAp4OhEbzQWigAFKOAAVmYKlZOwVxwY+KYLN3mv5oLRRYOCjDTIFMES",
    "bPpWXzoEYpjgAEY4NoNA4X6U3Ig2KADXT09PT09PXXXg",
).joinToString("")
