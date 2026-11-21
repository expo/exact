package dev.exact.canvas

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Binder
import android.os.ParcelFileDescriptor
import android.os.Process
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.nio.file.FileVisitResult
import java.nio.file.Path
import java.nio.file.LinkOption
import java.nio.file.SimpleFileVisitor
import java.nio.file.StandardOpenOption
import java.nio.file.attribute.BasicFileAttributes

/**
 * The app's files as content URIs another app may read under a grant (LLP 1133.024 D22 item 2, LLP 1094.000 D7):
 * what a drag, a share or a mail carries, as `androidx.core`'s `FileProvider` serves them. A URI names its source,
 * never a path: `app/data/…` for an `app:/data|cache|tmp/…` file, `shipped/assets/…` for one the app ships,
 * `capture/<id>/<name>` for the camera app's photo ([capture]). Each open reads the source as it is then: an `app:/`
 * file through the read the app's own `fs.read` makes (`Native.openApp`), below its root and through no symbolic link,
 * and a shipped one under the running generation's assets. Unexported, so only a URI the app granted with its intent
 * opens, and the grant is Android's. Only a capture's URI opens for writing, while its capture lasts. `AndroidManifest.xml`
 * declares it as `<package>.exact.files`.
 */
class Files : ContentProvider() {
    override fun onCreate() = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        val context = context ?: throw FileNotFoundException()
        val parts = parts(uri)
        if (parts.firstOrNull() == CAPTURE) {
            // Android checks only the write grant for a mode with `w`: reading as well needs the read grant too.
            if ('r' in mode && 'w' in mode && Binder.getCallingUid() != Process.myUid() &&
                context.checkCallingUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) != PackageManager.PERMISSION_GRANTED) {
                throw SecurityException("$mode needs a read grant as well as a write grant")
            }
            return captured(context, parts, mode) ?: throw FileNotFoundException()
        }
        if (mode != "r") throw SecurityException("the app's files are shared for reading only")
        return read(context, parts) ?: throw FileNotFoundException()
    }

    /** The name and size a receiver shows ([OpenableColumns]): the size the file has as it opens now. */
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, order: String?): Cursor {
        val context = context ?: throw FileNotFoundException()
        val parts = parts(uri)
        val opened = if (parts.firstOrNull() == CAPTURE) captured(context, parts, "r") else read(context, parts)
        val size = opened?.use { it.statSize } ?: throw FileNotFoundException()
        val columns = projection?.filter { it == OpenableColumns.DISPLAY_NAME || it == OpenableColumns.SIZE }?.toTypedArray()
            ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns, 1).apply { addRow(columns.map { if (it == OpenableColumns.SIZE) size else parts.last() }) }
    }

    override fun getType(uri: Uri): String = type(uri.lastPathSegment ?: "")

    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw UnsupportedOperationException()
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?): Int = throw UnsupportedOperationException()
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?): Int = throw UnsupportedOperationException()

    /** A file the camera app writes its photo or video into ([capture]), and the URI it writes through. */
    class Capture(val file: File, val uri: Uri)

    companion object {
        private const val APP = "app"
        private const val SHIPPED = "shipped"
        private const val CAPTURE = "capture"

        /**
         * The source [parts] name, open for reading: an `app:/` file through the app's own confined read, a shipped one
         * below the running generation's `assets` directory; null where it names no regular file now. In a process
         * whose reader has not booted, as one Android starts for the provider alone, no `app:/` root is known, and the
         * generation is the installed APK's.
         */
        private fun read(context: Context, parts: List<String>): ParcelFileDescriptor? {
            if (!named(parts)) return null
            return when (parts[0]) {
                APP -> if (parts[1] in ROOTS) Native.openApp("app:/" + parts.drop(1).joinToString("/")) else null
                SHIPPED -> if (parts[1] == "assets") shipped(context, parts.drop(2))?.let {
                    try { ParcelFileDescriptor.open(it, ParcelFileDescriptor.MODE_READ_ONLY) } catch (e: FileNotFoundException) { null }
                } else null
                else -> null
            }
        }

        /** The regular file [names] name below the running generation's `assets` directory, its links resolved; none
         * where `assets` is itself a link. */
        private fun shipped(context: Context, names: List<String>): File? = try {
            val directory = File(AssetRoot.of(context), "assets")
            if (!java.nio.file.Files.isDirectory(directory.toPath(), LinkOption.NOFOLLOW_LINKS)) null
            else {
                val assets = directory.canonicalFile
                File(assets, names.joinToString("/")).canonicalFile.takeIf { it.path.startsWith(assets.path + File.separator) && regular(it) }
            }
        } catch (e: IOException) {
            null
        }

        /** [uri]'s path as names, or none where it is no plain run of them: an empty part, a trailing separator. */
        private fun parts(uri: Uri): List<String> {
            val path = uri.encodedPath ?: return emptyList()
            return if (path.endsWith("/") || "//" in path) emptyList() else uri.pathSegments
        }

        /** Whether each of [parts] is one name: none empty, `.`, `..`, or holding a separator a URI encoded. */
        private fun named(parts: List<String>) =
            parts.size >= 2 && parts.none { it.isEmpty() || it == "." || it == ".." || '/' in it || '\u0000' in it }

        /** The capture [parts] names, opened as [mode] says, while its capture lasts: its directory is the reader's own,
         * which nothing of the app's reaches, and the file is the one [capture] made, so the mode makes none. Only the
         * modes `ContentResolver` documents open it: Android checks the write grant for a mode with `w` alone, so
         * `rt`, which empties the file, would pass with the read grant. */
        private fun captured(context: Context, parts: List<String>, mode: String): ParcelFileDescriptor? {
            if (parts.size != 3 || !named(parts) || mode !in MODES) return null
            val file = File(File(captures(context), parts[1]), parts[2]).takeIf(::regular) ?: return null
            return ParcelFileDescriptor.open(file, ParcelFileDescriptor.parseMode(mode) and ParcelFileDescriptor.MODE_CREATE.inv())
        }

        private val ROOTS = setOf("data", "cache", "tmp")
        private val MODES = setOf("r", "w", "wt", "wa", "rw", "rwt")

        /** The content URI for [src], an `app:/data|cache|tmp/…` path or a shipped `assets/…` name, or null where it
         * names no regular file the app has now. */
        fun uri(context: Context, src: String): Uri? {
            val parts = if (src.startsWith("app:/")) listOf(APP) + src.removePrefix("app:/").split('/') else listOf(SHIPPED) + src.split('/')
            read(context, parts)?.close() ?: return null
            return Uri.Builder().scheme("content").authority(authority(context)).apply { parts.forEach(::appendPath) }.build()
        }

        private fun authority(context: Context) = "${context.packageName}.exact.files"

        /** Where every capture's directory is: in the reader's cache, outside `app:/cache` and `app:/tmp` (`cache/cache`,
         * `cache/temporary`), where the app's own code writes. */
        fun captures(context: Context) = File(context.cacheDir, "exact.camera-app")

        /** A new, empty file named [name] for the camera app to write (`EXTRA_OUTPUT`), in a directory of its own named
         * at random, and its URI, which opens for writing until [endCapture]; null where it cannot be made. */
        fun capture(context: Context, name: String): Capture? {
            val id = java.util.UUID.randomUUID().toString()
            val file = File(File(captures(context), id), name)
            val made = try { file.parentFile!!.mkdirs() && file.createNewFile() } catch (e: IOException) { false }
            if (!made) return null
            return Capture(file, Uri.Builder().scheme("content").authority(authority(context)).appendPath(CAPTURE).appendPath(id).appendPath(name).build())
        }

        /** No app opens [capture]'s URI from now: Android's grants of it are taken back, while its file stays to be read. */
        fun revoke(context: Context, capture: Capture) =
            context.revokeUriPermission(capture.uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)

        /** [capture] over: [revoke]d, and its directory removed, so that its URI names nothing. */
        fun endCapture(context: Context, capture: Capture) {
            revoke(context, capture)
            capture.file.parentFile?.let(::removeTree)
        }

        /** A file's type by its name's extension, as a receiver reads it. */
        fun type(name: String): String =
            MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase()) ?: "application/octet-stream"

        /** [f]'s type by its extension, as a receiver reads it. */
        fun type(f: File): String = type(f.name)

        /** Removes [f] and, if it is a directory, everything in it, following no symbolic link: a link is removed, never
         * what it names. A module empties a directory of the app's with this, never [File.deleteRecursively], which
         * lists a linked directory and deletes its contents: a link the app's own code made under `app:/tmp` could name
         * any of the app's files. What cannot be removed is left. */
        fun removeTree(f: File) {
            fun remove(p: Path) { try { java.nio.file.Files.deleteIfExists(p) } catch (e: IOException) { /* left */ } }
            try {
                // Without FOLLOW_LINKS the walk visits a link as a file and does not enter it.
                java.nio.file.Files.walkFileTree(f.toPath(), object : SimpleFileVisitor<Path>() {
                    override fun visitFile(file: Path, attrs: BasicFileAttributes): FileVisitResult { remove(file); return FileVisitResult.CONTINUE }
                    override fun visitFileFailed(file: Path, exc: IOException): FileVisitResult { remove(file); return FileVisitResult.CONTINUE }
                    override fun postVisitDirectory(dir: Path, exc: IOException?): FileVisitResult { remove(dir); return FileVisitResult.CONTINUE }
                })
            } catch (e: IOException) { /* what is left stays */ }
        }

        /** [f] open for writing, or appending, refusing with an [IOException] where [f] is a symbolic link
         * (`O_NOFOLLOW`), as [File.outputStream] would open what it names. A module writes a file of the app's through
         * this or [write]: the app's own code may have made the name a link, even after the context answered it. */
        fun output(f: File, append: Boolean = false): java.io.OutputStream {
            val mode = if (append) StandardOpenOption.APPEND else StandardOpenOption.TRUNCATE_EXISTING
            return java.nio.file.Files.newOutputStream(f.toPath(), StandardOpenOption.CREATE, StandardOpenOption.WRITE, mode, LinkOption.NOFOLLOW_LINKS)
        }

        /** Writes [bytes] to [f], or appends them, through [output]. */
        fun write(f: File, bytes: ByteArray, append: Boolean = false) = output(f, append).use { it.write(bytes) }

        /** Whether [f] is a regular file, not a symbolic link to one. */
        fun regular(f: File): Boolean = java.nio.file.Files.isRegularFile(f.toPath(), LinkOption.NOFOLLOW_LINKS)
    }
}
