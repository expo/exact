// @exact/photos on Android (LLP 1133.019): MediaStore's images read and added to. Under the agent it asks MediaStore
// for nothing and answers from the agent's library (AgentLibrary.kt, §10); the runner holds the permission tickets and
// lays the status it keeps over each answer.
//
// - A photo reaches the app as an id (its MediaStore content URI), a picture (`photos:<id>`, which an `img` draws
//   through the reader's image door, `ContentResolver.loadThumbnail`, D2) or a copy in `app:/tmp/photos/` (emptied
//   when the module is first made in a launch).
// - The read permissions are by API level (D4): `READ_EXTERNAL_STORAGE` to 32, `READ_MEDIA_IMAGES` from 33, with
//   `READ_MEDIA_VISUAL_USER_SELECTED` from 34 (limited access), each asked only where the manifest declares it, with
//   `ACCESS_MEDIA_LOCATION` in the same request. Adding asks nothing (API 29 and later).
// - The app's own photos are read under the add line (D6): the runner lets a picture or a copy through as `owned`,
//   and MediaStore decides by opening the id.
// - Android has no event for a change in Settings, and the module observes none in MediaStore (LLP 1133.019.000 D1):
//   each resume reads the status again, announces it when it changed and the library's readers under a grant, and
//   revokes the pictures it drew that the app may no longer read (D6).
// - `photoChanges` is rescan-only (LLP 1133.019.000 D3): a token answers `expired`, and the app lists again.
// - Tier 3 (D9): `queryArgsHatch` sees each `photoAssets` query's `Bundle` after the module's arguments, on the
//   module's thread, and a page's key is joined to its selection after; `insertValuesHatch` each add's
//   `ContentValues`, likewise.
package exact.photos

import android.Manifest
import android.content.ContentUris
import android.content.ContentValues
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.util.Size
import dev.exact.canvas.Cause
import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

class PhotosModule(context: ExtensionContext) : ExactExtension(context) {
    companion object {
        /** The app's access hatches (D9), on the module's thread. */
        @JvmStatic var queryArgsHatch: ((Bundle) -> Unit)? = null
        @JvmStatic var insertValuesHatch: ((ContentValues) -> Unit)? = null
        private const val SELECTED = "android.permission.READ_MEDIA_VISUAL_USER_SELECTED"
        private val IMAGES = MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        /** Emptied once a launch. */
        private var emptied = false
    }

    private val app get() = context.application
    private val main = Handler(Looper.getMainLooper())
    /** The module's thread: queries, thumbnails, copies and adds. */
    private val work = Executors.newSingleThreadExecutor()
    private val library by lazy { AgentLibrary(app.assets) }
    /** The status last answered or read, which a resume compares with. */
    private var said: String? = null
    /** The ids whose pictures were drawn, which a narrowed access may revoke. */
    private val drawn = LinkedHashSet<String>()

    init {
        if (!emptied) {
            emptied = true
            context.directory("app:/tmp/photos")?.let { dev.exact.canvas.Files.removeTree(it) }
        }
        if (context.agent) library.missing?.let { say(it) }
    }

    private fun say(line: String) = context.log("photos: $line")
    private fun error(code: String, message: String? = null) = JSONObject().put("error", code).apply { if (message != null) put("message", message) }

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) = later(name, args, null, Cause.none, reply)

    override fun later(name: String, args: JSONObject, preferred: Any?, cause: Cause, reply: (JSONObject) -> Unit) {
        if (context.agent) return agent(name, args, reply)
        when (name) {
            "photoLibraryStatus" -> reply(status())
            "photoAssets" -> readable(cause, reply) { work.execute { reply(assets(args)) } }
            "photoChanges" -> readable(cause, reply) { reply(changes(args)) }
            "copyPhotoAsset" -> copy(args, cause, reply)
            "addToPhotoLibrary" -> work.execute { reply(add(args)) }
            "managePhotoSelection" -> manage(cause, reply)
            else -> reply(error("NotSupportedError"))
        }
    }

    /** The module observes no MediaStore change (LLP 1133.019.000 D1), and a selection can change in Settings with
     * the same status, so each resume under a grant announces the readers: an app replaces its index then. Otherwise
     * they are announced only when the read itself changed: a read announced while `prompt` asks again, so the resume
     * after a person's refusal would show the dialog a second time. */
    override fun resumed() {
        if (context.agent) return
        val last = said
        val now = status()
        if (last != null) {
            if (now.toString() != last) context.changed("photos.photoLibraryStatus")
            val read = { status: JSONObject -> status.getString("state") to status.getString("access") }
            if (now.getString("state") == "granted" || read(now) != read(JSONObject(last))) {
                context.changed("photos.photoAssets")
                context.changed("photos.photoChanges")
            }
        }
        // A picture can be drawn from an id the app kept, with no status read before.
        if (now.getString("access") != "full") withdraw()
    }

    /** What the app may read narrowed: a picture of a photo it can no longer open is withdrawn (D6). */
    private fun withdraw() {
        val ids = drawn.toList()
        work.execute {
            val gone = ids.filter { unreadable(Uri.parse(it)) }
            main.post { gone.forEach { drawn.remove(it); context.revokePicture("photos:$it") } }
        }
    }

    // The fact (D5)

    private fun has(permission: String) = app.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    private val declared: Set<String> by lazy {
        app.packageManager.getPackageInfo(app.packageName, PackageManager.GET_PERMISSIONS).requestedPermissions.orEmpty().toSet()
    }

    /** The read permissions this API level has, the full one first, as the manifest declares them. */
    private val reads: List<String>
        get() = when {
            Build.VERSION.SDK_INT >= 34 -> listOf(Manifest.permission.READ_MEDIA_IMAGES, SELECTED)
            Build.VERSION.SDK_INT >= 33 -> listOf(Manifest.permission.READ_MEDIA_IMAGES)
            else -> listOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        }.filter { it in declared }

    /** `full`, `limited` or `none`, from the permissions alone. */
    private fun access(): String {
        val full = reads.firstOrNull()?.let { it != SELECTED && has(it) } == true
        return if (full) "full" else if (SELECTED in reads && has(SELECTED)) "limited" else "none"
    }

    private fun status(): JSONObject {
        val access = access()
        val (full, limited) = (access == "full") to (access == "limited")
        val granted = full || limited
        // Neither: the state is the host's history of the request (LLP 1133.008 D8 item 7).
        val history = reads.firstOrNull()?.let { context.permission(it) }
        val state = if (granted) "granted" else history?.state ?: "prompt"
        val out = JSONObject()
            .put("available", true)
            .put("state", state)
            .put("access", access)
            .put("addState", "granted")
            .put("apple", JSONObject.NULL)
            .put("android", JSONObject()
                .put("shouldShowRationale", history?.shouldShowRationale == true)
                .put("permanentlyDenied", !granted && history?.permanentlyDenied == true)
                .put("generation", try { MediaStore.getVersion(app) } catch (e: RuntimeException) { JSONObject.NULL }))
        said = out.toString()
        return out
    }

    /** [then] once the app may read: asked in one request while `prompt`, by the app's own code or a person only (D7);
     * refused while `denied` or undeclared. */
    private fun readable(cause: Cause, reply: (JSONObject) -> Unit, then: () -> Unit) {
        val state = status()
        when {
            reads.isEmpty() -> reply(error("NotAllowedError", "the manifest declares no read permission"))
            state.getString("state") == "granted" -> then()
            state.getString("state") == "denied" -> reply(error("NotAllowedError"))
            !cause.app -> reply(error("NotAllowedError", "person-required"))
            else -> ask(cause) { if (status().getString("state") == "granted") then() else reply(error("NotAllowedError")) }
        }
    }

    /** The platform's dialog for the read permissions and the location one, then [done]. */
    private fun ask(cause: Cause, done: () -> Unit) {
        val location = listOf(Manifest.permission.ACCESS_MEDIA_LOCATION).filter { it in declared }
        val asked = (reads + location).toTypedArray()
        val first = reads.first()
        context.requestPermissions(cause, asked) { answered ->
            // A refusal with no rationale owed after it is permanent: the history `permission` reads, as
            // `askPermission` keeps it for one permission.
            val activity = context.activity
            if (first in answered && status().getString("state") != "granted" && activity != null && !activity.shouldShowRequestPermissionRationale(first)) {
                app.getSharedPreferences("exact.permissions", android.content.Context.MODE_PRIVATE).edit().putBoolean(first, true).apply()
            }
            context.changed("photos.photoLibraryStatus")
            done()
        }
    }

    /** API 34's selection dialog: the read request again, while access is limited (D6). Answers the status. The dialog
     * can answer with the activity still resumed, and a limited library changed within itself reads the same status,
     * so the pictures of photos it took out are withdrawn here, not at a resume. */
    private fun manage(cause: Cause, reply: (JSONObject) -> Unit) {
        if (Build.VERSION.SDK_INT < 34 || status().getString("access") != "limited") return reply(status())
        context.requestPermissions(cause, reads.toTypedArray()) {
            context.changed("photos.photoLibraryStatus")
            context.changed("photos.photoAssets")
            context.changed("photos.photoChanges")
            val now = status()
            if (now.getString("access") != "full") withdraw()
            reply(now)
        }
    }

    // Reads

    private val columns = arrayOf(
        MediaStore.Images.Media._ID, MediaStore.Images.Media.WIDTH, MediaStore.Images.Media.HEIGHT,
        MediaStore.Images.Media.ORIENTATION, MediaStore.Images.Media.DATE_TAKEN, MediaStore.Images.Media.DATE_ADDED,
        MediaStore.Images.Media.RELATIVE_PATH, MediaStore.Images.Media.BUCKET_DISPLAY_NAME,
        MediaStore.Images.Media.OWNER_PACKAGE_NAME, MediaStore.Images.Media.MIME_TYPE, MediaStore.Images.Media.DISPLAY_NAME,
    ) + if (Build.VERSION.SDK_INT >= 30) arrayOf(MediaStore.MediaColumns.IS_FAVORITE) else emptyArray()

    /** `photoAssets`. No cursor, or a failure of the provider or of the app's hatch, answers an error, never an empty
     * last page, which would let a replacement drop the app's index (LLP 1133.019.000 D3). */
    private fun assets(args: JSONObject): JSONObject = try {
        page(args)
    } catch (e: SecurityException) {
        failed("NotAllowedError", e.javaClass.name)
    } catch (e: android.os.OperationCanceledException) {
        failed("AbortError", e.javaClass.name)
    } catch (e: RuntimeException) { failed("UnknownError", e.javaClass.name) }

    /** A failure of `photoAssets`, logged by its names alone: no ids, no selection, no provider message. */
    private fun failed(code: String, cause: String): JSONObject {
        say("photoAssets: $code ($cause)")
        return error(code)
    }

    private fun page(args: JSONObject): JSONObject {
        val limit = args.optInt("limit", 60).coerceIn(1, 500)
        // The key's numbers: the date taken and the date added, either null, and the row.
        val after = try {
            PageCursor.key("mediastore", args.opt("cursor"), 3)?.map { it?.toLong() }
        } catch (e: IllegalArgumentException) { return PageCursor.refused() }
        // Newest first: the date taken, else the date added (seconds, so many share one), then the row. A null sorts
        // last, as SQLite orders it lowest. One row past the page says whether an older page follows.
        val sort = "${MediaStore.Images.Media.DATE_TAKEN} DESC, ${MediaStore.Images.Media.DATE_ADDED} DESC, ${MediaStore.Images.Media._ID} DESC"
        val fetched = limit + 1
        val query = Bundle().apply {
            putString(android.content.ContentResolver.QUERY_ARG_SQL_SORT_ORDER, sort)
            putInt(android.content.ContentResolver.QUERY_ARG_LIMIT, fetched)
        }
        queryArgsHatch?.let { hatch ->
            hatch(query)
            if (query.getString(android.content.ContentResolver.QUERY_ARG_SQL_SORT_ORDER) != sort) {
                say("the queryArgs hatch changed the sort order, which the module owns (LLP 1133.019 D9)")
                query.putString(android.content.ContentResolver.QUERY_ARG_SQL_SORT_ORDER, sort)
            }
            if (query.getInt(android.content.ContentResolver.QUERY_ARG_LIMIT, -1) != fetched) {
                say("the queryArgs hatch changed the limit, which the module owns (LLP 1133.019 D9)")
                query.putInt(android.content.ContentResolver.QUERY_ARG_LIMIT, fetched)
            }
        }
        // The subtype filter and the page's key join the app's selection; the key's values are numbers the cursor
        // held, written as literals, so the app's selection arguments keep their places.
        val keys = listOf(MediaStore.Images.Media.DATE_TAKEN, MediaStore.Images.Media.DATE_ADDED, MediaStore.Images.Media._ID)
        val ours = listOfNotNull(subtypeSelection(args), after?.let { olderThan(keys.zip(it)) })
        if (ours.isNotEmpty()) {
            val theirs = query.getString(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION)
            val joined = (listOfNotNull(theirs?.ifBlank { null }) + ours).joinToString(" AND ") { "($it)" }
            query.putString(android.content.ContentResolver.QUERY_ARG_SQL_SELECTION, joined)
        }
        val out = JSONArray()
        var last: List<Long?>? = null
        var more = false
        val c = app.contentResolver.query(IMAGES, columns, query, null) ?: return failed("UnknownError", "no cursor")
        c.use {
            while (c.moveToNext()) {
                if (out.length() == limit) { more = true; break }
                out.put(asset(c))
                last = listOf(if (c.isNull(4)) null else c.getLong(4), if (c.isNull(5)) null else c.getLong(5), c.getLong(0))
            }
        }
        val cursor = last?.takeIf { more }?.let { PageCursor.text("mediastore", it.map { v -> v?.toString() }) }
        return JSONObject().put("assets", out).put("cursor", cursor ?: JSONObject.NULL)
    }

    /** `subtypes`' selection (LLP 1133.019.000 D4), null for every image: `screenshot` is a row whose `RELATIVE_PATH`'s
     * last nonempty segment is the screenshots directory (`Pictures/Screenshots/`, `DCIM/Screenshots/`), in any case,
     * as SQLite's `LIKE` compares. */
    private fun subtypeSelection(args: JSONObject): String? {
        if (!screenshots(args)) return null
        val (path, dir) = "rtrim(${MediaStore.Images.Media.RELATIVE_PATH}, '/')" to android.os.Environment.DIRECTORY_SCREENSHOTS
        return "$path LIKE '$dir' OR $path LIKE '%/$dir'"
    }

    private fun screenshots(args: JSONObject): Boolean =
        args.optJSONArray("subtypes")?.let { a -> (0 until a.length()).any { a.optString(it) == "screenshot" } } == true

    /** What changed since `since` (LLP 1133.019.000 D1, D3): every view is rescan-only on Android, so a token answers
     * `expired` with a fresh one, and the app replaces its index from `photoAssets`. */
    private fun changes(args: JSONObject): JSONObject {
        val here = ChangeToken("mediastore", access(), ChangeToken.subtypes(args))
        val since = try { ChangeToken.parse(args.opt("since"), "mediastore") } catch (e: IllegalArgumentException) { return ChangeToken.refused() }
        // Every token this answers is a rescan-only one.
        return when {
            since == null -> here.answer()
            since.rescan && since.epoch == null -> here.answer(expired = true)
            else -> ChangeToken.refused()
        }
    }

    /** What sorts after [key] under a sort of its columns, each descending with a null last; the last is never null. */
    private fun olderThan(key: List<Pair<String, Long?>>): String {
        val (column, value) = key.first()
        if (key.size == 1) return "$column < $value"
        val rest = olderThan(key.drop(1))
        return if (value == null) "($column IS NULL AND $rest)"
        else "($column < $value OR $column IS NULL OR ($column = $value AND $rest))"
    }

    private fun asset(c: android.database.Cursor): JSONObject {
        val id = ContentUris.withAppendedId(IMAGES, c.getLong(0)).toString()
        val turned = c.getInt(3) % 180 != 0
        val (w, h) = c.getInt(1) to c.getInt(2)
        val taken = if (c.isNull(4)) null else c.getLong(4).toDouble()
        val text = { i: Int -> if (c.isNull(i)) JSONObject.NULL else c.getString(i) }
        return JSONObject()
            .put("id", id)
            .put("mediaType", "image")
            .put("width", if (turned) h else w)
            .put("height", if (turned) w else h)
            .put("creationDate", taken ?: (c.getLong(5) * 1000.0))
            .put("favorite", c.columnCount > 11 && c.getInt(11) == 1)
            .put("picture", "photos:$id")
            .put("apple", JSONObject.NULL)
            .put("android", JSONObject().put("relativePath", text(6)).put("bucketDisplayName", text(7)).put("ownerPackageName", text(8)))
    }

    /** Whether the app may no longer open [uri]: refused, or no row. A query that fails otherwise, or answers no
     * cursor, is no answer, and the picture stays. */
    private fun unreadable(uri: Uri): Boolean = try {
        app.contentResolver.query(uri, arrayOf(MediaStore.Images.Media._ID), null, null, null)?.use { !it.moveToFirst() } ?: false
    } catch (e: SecurityException) { true } catch (e: RuntimeException) { false }

    /** Whether the app may open [uri] now. */
    private fun opens(uri: Uri): Boolean = try {
        app.contentResolver.query(uri, arrayOf(MediaStore.Images.Media._ID), null, null, null)?.use { it.moveToFirst() } == true
    } catch (e: SecurityException) { false }

    override fun picture(source: String, width: Int, height: Int, fit: String, owned: Boolean, reply: (Bitmap?, String?) -> Unit) {
        val id = source.removePrefix("photos:")
        if (context.agent) return agentPicture(id, width, height, fit, owned, reply)
        val uri = Uri.parse(id)
        work.execute {
            try {
                val size = app.contentResolver.query(uri, arrayOf(MediaStore.Images.Media.WIDTH, MediaStore.Images.Media.HEIGHT), null, null, null)
                    ?.use { if (it.moveToFirst()) it.getInt(0) to it.getInt(1) else null }
                    ?: return@execute reply(null, "NotFoundError")
                val longest = longest(size, width, height, fit)
                val bitmap = app.contentResolver.loadThumbnail(uri, Size(longest, longest), null)
                main.post { drawn += id }
                reply(bitmap, null)
            } catch (e: SecurityException) {
                // Another app's photo under the add line alone (D6): an unresolved image.
                reply(null, if (owned) "NotFoundError" else "NotAllowedError")
            } catch (e: java.io.IOException) {
                reply(null, "NotFoundError")
            }
        }
    }

    /** The longest side a picture of [size] needs for a box of [width] by [height] at [fit]: no larger than it is. */
    private fun longest(size: Pair<Int, Int>, width: Int, height: Int, fit: String): Int {
        val (w, h) = size.first.toDouble().coerceAtLeast(1.0) to size.second.toDouble().coerceAtLeast(1.0)
        val factor = if (fit == "contain" || fit == "scale-down") min(width / w, height / h) else max(width / w, height / h)
        return max(1, min(max(w, h), ceil(max(w, h) * factor)).toInt())
    }

    // Copies and adds

    private fun copy(args: JSONObject, cause: Cause, reply: (JSONObject) -> Unit) {
        val uri = Uri.parse(args.optString("asset"))
        // Opened as the app's own when it is (D6); else a read of the library, asked while `prompt`.
        work.execute {
            if (opens(uri)) return@execute reply(copied(uri, args))
            main.post {
                if (reads.isEmpty()) return@post reply(error("NotFoundError"))
                readable(cause, reply) { work.execute { reply(if (opens(uri)) copied(uri, args) else error("NotFoundError")) } }
            }
        }
    }

    private fun copied(uri: Uri, args: JSONObject): JSONObject {
        val row = app.contentResolver.query(uri, columns, null, null, null)?.use { if (it.moveToFirst()) asset(it).put("name", it.getString(10)).put("type", it.getString(9)) else null }
            ?: return error("NotFoundError")
        val name = row.optString("name")
        // The file's own extension, else its type's, as Apple's copy takes it.
        val ext = name.substringAfterLast('.', "").lowercase()
            .ifEmpty { android.webkit.MimeTypeMap.getSingleton().getExtensionFromMimeType(row.optString("type")) ?: "img" }
        val path = destination(args, ext)
        val file = context.file(path) ?: return error("NotAllowedError")
        // The original bytes, location kept, where the app holds ACCESS_MEDIA_LOCATION (D4); the owner's are never
        // redacted, so the flag describes another app's photo opened without it.
        val original = has(Manifest.permission.ACCESS_MEDIA_LOCATION)
        val own = row.getJSONObject("android").opt("ownerPackageName") == app.packageName
        val size = try {
            file.parentFile?.mkdirs()
            app.contentResolver.openInputStream(if (original) MediaStore.setRequireOriginal(uri) else uri)?.use { input ->
                dev.exact.canvas.Files.output(file).use { input.copyTo(it) }
            } ?: return error("NotFoundError")
        } catch (e: SecurityException) { return error("NotAllowedError") } catch (e: java.io.IOException) { return error("UnknownError") }
        return JSONObject()
            .put("path", path).put("name", name).put("type", row.optString("type")).put("size", size)
            .put("width", row.getInt("width")).put("height", row.getInt("height")).put("duration", JSONObject.NULL)
            .put("android", JSONObject().put("locationRedacted", !original && !own))
    }

    /** Where a copy goes: the app's `path` with [ext], else a name of its own, an uppercase UUID as Apple's copy has,
     * so no copy replaces another still in use (D2 item 3). */
    private fun destination(args: JSONObject, ext: String): String =
        args.optString("path").ifEmpty { null }?.let { if (it.endsWith(".$ext")) it else "$it.$ext" }
            ?: "app:/tmp/photos/${java.util.UUID.randomUUID().toString().uppercase()}.$ext"

    private fun add(args: JSONObject): JSONObject {
        val path = args.optString("path")
        val file = context.fileToRead(path) ?: return error("NotFoundError")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        val type = bounds.outMimeType ?: return error("DataError")
        val values = ContentValues().apply {
            put(MediaStore.Images.Media.DISPLAY_NAME, file.name)
            put(MediaStore.Images.Media.MIME_TYPE, type)
            put(MediaStore.Images.Media.IS_PENDING, 1)
            args.optJSONObject("android")?.optString("relativePath")?.ifEmpty { null }?.let { put(MediaStore.Images.Media.RELATIVE_PATH, it) }
        }
        insertValuesHatch?.invoke(values)
        val resolver = app.contentResolver
        val uri = try { resolver.insert(IMAGES, values) } catch (e: IllegalArgumentException) { return error("DataError", e.message) } ?: return error("UnknownError")
        try {
            resolver.openOutputStream(uri)?.use { out -> file.inputStream().use { it.copyTo(out) } } ?: throw java.io.IOException("no stream")
            resolver.update(uri, ContentValues().apply { put(MediaStore.Images.Media.IS_PENDING, 0) }, null, null)
        } catch (e: java.io.IOException) {
            resolver.delete(uri, null, null)
            return error("UnknownError")
        }
        main.post {
            context.changed("photos.photoAssets")
            context.changed("photos.photoChanges")
        }
        // The app reads its own photo whatever its read permission (D6).
        return JSONObject().put("asset", uri.toString())
    }

    // The agent's answers (§10)

    private fun agent(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        when (name) {
            "photoLibraryStatus", "managePhotoSelection" -> reply(agentStatus())
            "photoAssets" -> reply(agentAssets(args))
            "photoChanges" -> reply(agentChanges(args))
            "copyPhotoAsset" -> reply(agentCopy(args))
            "addToPhotoLibrary" -> reply(agentAdd(args))
            else -> reply(error("NotSupportedError"))
        }
    }

    /** Under the agent: the access the runner's status gives and the selection; a photo that left what the app may
     * read is revoked. */
    override fun preferred(values: JSONObject) {
        if (!context.agent) return
        val strings = { k: String -> values.optJSONArray(k)?.let { a -> (0 until a.length()).map { a.getString(it) } } }
        change {
            values.optJSONObject("photoLibraryStatus")?.optString("access")?.ifEmpty { null }?.let { access = it }
            selection = strings("selection") ?: AgentLibrary.defaultSelection
            deleted = strings("deleted").orEmpty().toSet()
        }
    }

    override fun permissions(states: JSONObject) {
        // Denied reading: nothing is readable, whatever the status said before.
        if (context.agent && states.optString("photos") == "denied") change { access = "none" }
    }

    /** The agent's library changed by [edit]: a photo that left what the app may read is revoked, and the readers of
     * what moved are announced. */
    private fun change(edit: AgentLibrary.() -> Unit) {
        val seen = library.visible
        val moved = Triple(library.access, library.epoch, library.journal.size)
        library.edit()
        val after = library.visible
        for (id in (seen - after).sorted()) {
            say("revoked photos:$id")
            context.revokePicture("photos:$id")
        }
        if (seen != after) context.changed("photos.photoAssets")
        if (seen != after || moved != Triple(library.access, library.epoch, library.journal.size)) context.changed("photos.photoChanges")
    }

    private fun agentStatus() = JSONObject()
        .put("available", true).put("state", "prompt").put("access", library.access).put("addState", "prompt")
        .put("apple", JSONObject.NULL).put("android", JSONObject.NULL)

    private fun agentAssets(args: JSONObject): JSONObject {
        val after = try { PageCursor.key("agent", args.opt("cursor"), 2) } catch (e: IllegalArgumentException) { return PageCursor.refused() }
        val created = after?.get(0)?.toDoubleOrNull()
        val (page, more) = library.page(args.optInt("limit", 60).coerceIn(1, 500), after?.let { (created ?: Double.NEGATIVE_INFINITY) to it[1]!! }, screenshots(args))
        val cursor = page.lastOrNull()?.takeIf { more }?.let { PageCursor.text("agent", listOf(it.created.toString(), it.id)) }
        return JSONObject().put("assets", JSONArray(page.map(::agentAsset))).put("cursor", cursor ?: JSONObject.NULL)
    }

    /** The agent's journal from `since` (LLP 1133.019.000 D1, D5). A walk fixes its end on its first page; each page
     * after takes up after the last photo the one before answered. */
    private fun agentChanges(args: JSONObject): JSONObject {
        val count = library.journal.size
        val here = ChangeToken("agent", library.access, ChangeToken.subtypes(args), library.epoch, count.toString())
        val since = try { ChangeToken.parse(args.opt("since"), "agent") } catch (e: IllegalArgumentException) { return ChangeToken.refused() }
            ?: return here.answer()
        // The agent answers no rescan-only token, and each of its tokens names its store and a position.
        val from = since.at?.toIntOrNull()
        val fixed = since.end?.let { it.toIntOrNull() ?: -1 }
        if (since.epoch == null || from == null || from < 0 || (fixed != null && fixed < from)) return ChangeToken.refused()
        if (!here.sameView(since) || since.epoch != library.epoch) return here.answer(expired = true)
        // Positions in this store: a walk's end, else now.
        val end = fixed ?: count
        if (from > end || end > count) return ChangeToken.refused()
        val page = library.changes(from, end, since.after, ChangeToken.limit(args), "screenshot" in here.subtypes) ?: return ChangeToken.refused()
        val next = if (page.more) here.copy(at = from.toString(), end = end.toString(), after = page.last) else here.copy(at = end.toString())
        return next.answer(page.upserts.filter { it.second }.map { agentAsset(it.first) }, page.upserts.filter { !it.second }.map { agentAsset(it.first) },
            page.removed, page.more)
    }

    private fun agentAsset(a: AgentLibrary.Asset) = JSONObject()
        .put("id", a.id).put("mediaType", "image").put("width", a.width).put("height", a.height).put("creationDate", a.created)
        .put("favorite", false).put("picture", "photos:${a.id}").put("apple", JSONObject.NULL)
        .put("android", JSONObject().put("relativePath", if (a.screenshot) "Pictures/Screenshots/" else JSONObject.NULL)
            .put("bucketDisplayName", if (a.screenshot) "Screenshots" else JSONObject.NULL)
            .put("ownerPackageName", if (a.own) app.packageName else JSONObject.NULL))

    private fun agentPicture(id: String, width: Int, height: Int, fit: String, owned: Boolean, reply: (Bitmap?, String?) -> Unit) {
        val asset = library.asset(id) ?: run {
            say("picture $id at ${width}x$height: NotFoundError")
            return reply(null, "NotFoundError")
        }
        say("picture $id at ${width}x$height $fit${if (owned) " owned" else ""}")
        work.execute {
            val bitmap = library.picture(asset, width, height, fit)
            reply(bitmap, if (bitmap == null) "UnknownError" else null)
        }
    }

    private fun agentCopy(args: JSONObject): JSONObject {
        val id = args.optString("asset")
        val asset = library.asset(id) ?: return error("NotFoundError")
        val path = destination(args, asset.name.substringAfterLast('.'))
        val file = context.file(path) ?: return error("UnknownError")
        try {
            file.parentFile?.mkdirs()
            dev.exact.canvas.Files.write(file, asset.bytes)
        } catch (e: java.io.IOException) { return error("UnknownError") }
        say("copied $id to $path sha256 ${AgentLibrary.sha256(asset.bytes)}")
        return JSONObject().put("path", path).put("name", asset.name).put("type", asset.type).put("size", asset.bytes.size)
            .put("width", asset.width).put("height", asset.height).put("duration", JSONObject.NULL)
            .put("android", JSONObject().put("locationRedacted", false))
    }

    private fun agentAdd(args: JSONObject): JSONObject {
        val path = args.optString("path")
        val bytes = context.fileToRead(path)?.readBytes() ?: return error("NotFoundError")
        val asset = library.add(bytes, context.now()) { context.file("app:/tmp/photos-library/$it") } ?: return error("DataError")
        say("added ${asset.id} from $path sha256 ${AgentLibrary.sha256(bytes)} ${asset.width}x${asset.height} ${asset.type}")
        context.changed("photos.photoAssets")
        context.changed("photos.photoChanges")
        // The app reads its own photo whatever its read permission (D6).
        return JSONObject().put("asset", asset.id)
    }
}

/** `photoAssets`' cursor (LLP 1133.019 D1), opaque to the app: the sort key of the last photo a page answered, tagged
 * with the library that sorted it. The next page answers what sorts after the key, so photos added newer do not move
 * it, and a key whose photo is gone still places it. */
internal object PageCursor {
    fun refused(): JSONObject = JSONObject().put("error", "SyntaxError").put("message", "not a cursor photoAssets answered here")

    fun text(library: String, key: List<String?>): String =
        android.util.Base64.encodeToString((listOf(library) + key.map { it ?: "" }).joinToString("\n").toByteArray(), android.util.Base64.NO_WRAP)

    /** The key's [size] values, an empty one null; null for the first page (no cursor, or an empty one). Throws
     * [IllegalArgumentException] for a cursor this library did not make. */
    fun key(library: String, given: Any?, size: Int): List<String?>? {
        val text = given as? String
        if (text.isNullOrEmpty()) return null
        val parts = String(android.util.Base64.decode(text, android.util.Base64.NO_WRAP)).split("\n")
        require(parts.size == size + 1 && parts[0] == library && parts.last().isNotEmpty())
        return parts.drop(1).map { it.ifEmpty { null } }
    }
}
