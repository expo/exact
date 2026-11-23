// The agent's photo library on Android (LLP 1133.019 §10), as Apple's `AgentLibrary.swift` is: what @exact/photos
// reads and adds to under the agent, where it asks MediaStore for nothing. Its photos are `fixture/agent-1.jpg` to
// `agent-6.jpg`, which an APK carries as the app's assets under `photos-fixture/` (the fixture app's `assets/`; the
// reader puts an app's assets at `app/assets/` in the APK), with fixed creation dates, agent-1 the newest; agent-3
// is stored turned (EXIF orientation 6), agent-5 is in Display P3, and agent-2 is marked a screenshot (LLP
// 1133.019.000 D4), listed under `Pictures/Screenshots/`. An add joins it, its file kept under the
// session's `app:/tmp/photos-library/`.
// What the app may see of it is the access the runner's status gives (`full`, `limited`, `none`) and, under
// `limited`, the selection (the agent's `selection`, else `agent-1` to `agent-3`), with the app's own adds, which
// Android lets an app read whatever its read permission (D6), less the fixtures the agent's `deleted` names. Its journal
// is what `photoChanges` walks (LLP 1133.019.000 D5).
package exact.photos

import android.content.res.AssetManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import java.io.ByteArrayInputStream
import java.io.File
import java.security.MessageDigest
import java.time.ZoneOffset
import java.time.ZonedDateTime
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

internal class AgentLibrary(assets: AssetManager) {
    class Asset(
        val id: String,
        val bytes: ByteArray,
        /** Its size as the gallery shows it, orientation applied. */
        val width: Int,
        val height: Int,
        /** Ms since the epoch. */
        val created: Double,
        /** Added by the app in this session. */
        val own: Boolean,
        val type: String,
        val name: String,
        val screenshot: Boolean = false,
    )

    companion object {
        val defaultSelection = listOf("agent-1", "agent-2", "agent-3")

        fun sha256(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

        /** [bytes]' upright size and media type, or null for bytes that are no image. */
        fun size(bytes: ByteArray): Triple<Int, Int, String>? {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            val type = bounds.outMimeType ?: return null
            if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
            val turned = orientation(bytes) >= 5
            return Triple(if (turned) bounds.outHeight else bounds.outWidth, if (turned) bounds.outWidth else bounds.outHeight, type)
        }

        private fun orientation(bytes: ByteArray): Int = try {
            ExifInterface(ByteArrayInputStream(bytes)).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)
        } catch (e: Exception) { ExifInterface.ORIENTATION_NORMAL }

        private fun extension(type: String) = when (type) { "image/png" -> "png"; "image/webp" -> "webp"; "image/heic" -> "heic"; else -> "jpg" }
    }

    val assets = ArrayList<Asset>()
    var access = "none"
    /** A new selection names a new store, so a token from before it expires. */
    var selection = defaultSelection
        set(value) {
            if (value.toSet() != field.toSet()) epoch = java.util.UUID.randomUUID().toString()
            field = value
        }
    /** The photos deleted, each journalled as it goes and as it comes back; an id the library never had is no photo to
     * delete. */
    var deleted: Set<String> = emptySet()
        set(given) {
            val value = given.intersect(assets.map { it.id }.toSet())
            journal += (value - field).sorted().map { it to false }
            journal += (field - value).sorted().map { it to true }
            field = value
        }
    /** Each change in order, and whether it inserted the photo; a token's position counts them. */
    val journal = ArrayList<Pair<String, Boolean>>()
    /** This store's name: each launch makes a new library, so a token from another launch expires. */
    var epoch: String = java.util.UUID.randomUUID().toString()
        private set
    /** Why the fixtures are missing, when they are. */
    var missing: String? = null

    init {
        // 2026-0(7-n)-1n 10:(7n):00 UTC, as the fixtures' EXIF dates say.
        for (n in 1..6) {
            val bytes = try { assets.open("app/assets/photos-fixture/agent-$n.jpg").use { it.readBytes() } } catch (e: java.io.IOException) {
                missing = "the APK carries no assets/photos-fixture/agent-$n.jpg"
                continue
            }
            val created = ZonedDateTime.of(2026, 7 - n, 10 + n, 10, 7 * n, 0, 0, ZoneOffset.UTC).toInstant().toEpochMilli().toDouble()
            val (w, h, type) = size(bytes) ?: continue
            this.assets += Asset("agent-$n", bytes, w, h, created, false, type, "agent-$n.jpg", screenshot = n == 2)
        }
    }

    /** The ids the app may read now. */
    val visible: Set<String>
        get() = when (access) {
            "full" -> assets.map { it.id }.toSet()
            "limited" -> selection.toSet() + assets.filter { it.own }.map { it.id }
            else -> assets.filter { it.own }.map { it.id }.toSet()
        } - deleted

    fun asset(id: String): Asset? = if (id in visible) assets.firstOrNull { it.id == id } else null

    /** Of what the app may read, the screenshots alone when [screenshots], newest first with the id ordering one date,
     * at most [limit] that sort after the key [after] (a creation date and an id), and whether more do. */
    fun page(limit: Int, after: Pair<Double, String>?, screenshots: Boolean = false): Pair<List<Asset>, Boolean> {
        val seen = visible
        val order = compareByDescending<Asset> { it.created }.thenByDescending { it.id }
        var rest = assets.filter { it.id in seen && (!screenshots || it.screenshot) }.sortedWith(order)
        if (after != null) rest = rest.filter { it.created < after.first || (it.created == after.first && it.id < after.second) }
        return rest.take(limit) to (rest.size > limit)
    }

    /** An add: [bytes], kept at the file [destination] answers for its name, join the library at the session's [now].
     * Null for bytes that are no image, or where [destination] answers no file or it is not written. */
    fun add(bytes: ByteArray, now: Double, destination: (String) -> File?): Asset? {
        val (w, h, type) = size(bytes) ?: return null
        val id = "agent-${assets.size + 1}"
        val name = "$id.${extension(type)}"
        val file = destination(name) ?: return null
        try {
            file.parentFile?.mkdirs()
            dev.exact.canvas.Files.write(file, bytes)
        } catch (e: java.io.IOException) { return null }
        // The session's clock counts from 0 under the agent, so an add is dated a day after the newest fixture, plus
        // that clock: newer than all of them, as a photo saved today is.
        val newest = assets.filter { !it.own }.maxOfOrNull { it.created } ?: 0.0
        val asset = Asset(id, bytes, w, h, newest + 86_400_000 + now, true, type, name)
        assets += asset
        journal += id to true
        return asset
    }

    class Changes(val upserts: List<Pair<Asset, Boolean>>, val removed: List<String>, val last: String?, val more: Boolean)

    /** One page of the journal's changes from [from] up to [end] (LLP 1133.019.000 D2's fold): each photo changed once,
     * by id after [after], at most [limit]. A photo the app may read in the view is an upsert, paired with whether its
     * first change inserted it; any other is a removal. Null when [after] is no photo the window changed, or its last,
     * which no page of it ended on. */
    fun changes(from: Int, end: Int, after: String?, limit: Int, screenshots: Boolean): Changes? {
        val first = LinkedHashMap<String, Boolean>()
        for ((id, insert) in journal.subList(from, end)) first.putIfAbsent(id, insert)
        if (after != null && (after !in first || first.keys.none { it > after })) return null
        val ids = first.keys.filter { after == null || it > after }.sorted()
        val page = ids.take(limit)
        val seen = visible
        val upserts = ArrayList<Pair<Asset, Boolean>>()
        val removed = ArrayList<String>()
        for (id in page) {
            val a = assets.firstOrNull { it.id == id }
            if (id in seen && a != null && (!screenshots || a.screenshot)) upserts += a to first.getValue(id) else removed += id
        }
        return Changes(upserts, removed, page.lastOrNull(), ids.size > page.size)
    }

    /** [asset] drawn upright for a box of [width] by [height] device pixels at [fit]: no larger than it is. */
    fun picture(asset: Asset, width: Int, height: Int, fit: String): Bitmap? {
        val (w, h) = asset.width.toDouble() to asset.height.toDouble()
        val (x, y) = width / w to height / h
        val factor = if (fit == "contain" || fit == "scale-down") min(x, y) else max(x, y)
        val scale = min(1.0, ceil(max(w, h) * factor) / max(w, h))
        // BitmapFactory never applies the EXIF orientation, so the turn below is always the file's own.
        val options = BitmapFactory.Options().apply {
            inSampleSize = Integer.highestOneBit(max(1, (1 / scale).toInt()))
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        val stored = BitmapFactory.decodeByteArray(asset.bytes, 0, asset.bytes.size, options) ?: return null
        val o = orientation(asset.bytes)
        val turned = o >= 5
        val (uw, uh) = if (turned) stored.height to stored.width else stored.width to stored.height
        val matrix = Matrix()
        when (o) {
            ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> matrix.setScale(-1f, 1f)
            ExifInterface.ORIENTATION_ROTATE_180 -> matrix.setRotate(180f)
            ExifInterface.ORIENTATION_FLIP_VERTICAL -> matrix.setScale(1f, -1f)
            ExifInterface.ORIENTATION_TRANSPOSE -> { matrix.setRotate(90f); matrix.postScale(-1f, 1f) }
            ExifInterface.ORIENTATION_ROTATE_90 -> matrix.setRotate(90f)
            ExifInterface.ORIENTATION_TRANSVERSE -> { matrix.setRotate(-90f); matrix.postScale(-1f, 1f) }
            ExifInterface.ORIENTATION_ROTATE_270 -> matrix.setRotate(-90f)
        }
        val target = max(1.0, ceil(max(w, h) * scale))
        val s = (target / max(uw, uh)).toFloat()
        matrix.postScale(s, s)
        return Bitmap.createBitmap(stored, 0, 0, stored.width, stored.height, matrix, true)
    }
}
