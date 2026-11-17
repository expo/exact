package dev.exact.canvas

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
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
 * The app's files as content URIs another app may read under a grant (LLP 1133.024 D22 item 2): what an intent carries
 * for a mail's attachments, as `androidx.core`'s `FileProvider` does, without the library. Unexported, so only a URI
 * the app granted with its intent opens, and only for reading, unless the app handed that one file out for writing
 * ([writableUri]: the camera app's photo); a path outside the app's own data directory opens nothing. [uri] makes
 * one; `AndroidManifest.xml` declares it as `<package>.exact.files`.
 */
class Files : ContentProvider() {
    override fun onCreate() = true

    private fun file(uri: Uri): File {
        val context = context ?: throw FileNotFoundException()
        val f = File(uri.path ?: "").canonicalFile
        if (!inside(context, f) || !f.isFile) throw FileNotFoundException()
        return f
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode == "r") return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY)
        // [file] resolves links, so a link put in place of a handed-out file names one that is not handed out.
        val f = file(uri)
        if (!writes(f)) throw SecurityException("the app's files are shared for reading only")
        return ParcelFileDescriptor.open(f, ParcelFileDescriptor.parseMode(mode))
    }

    /** The name and size a receiver shows ([OpenableColumns]). */
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, order: String?): Cursor {
        val f = file(uri)
        val columns = projection?.filter { it == OpenableColumns.DISPLAY_NAME || it == OpenableColumns.SIZE }?.toTypedArray()
            ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns, 1).apply { addRow(columns.map { if (it == OpenableColumns.SIZE) f.length() else f.name }) }
    }

    override fun getType(uri: Uri): String = type(File(uri.path ?: ""))

    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw UnsupportedOperationException()
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?): Int = throw UnsupportedOperationException()
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?): Int = throw UnsupportedOperationException()

    companion object {
        private fun inside(context: Context, f: File): Boolean = f.path.startsWith(context.dataDir.canonicalPath + File.separator)

        /** The content URI for [f], one of the app's own files, or null for any other. */
        fun uri(context: Context, f: File): Uri? {
            val canonical = f.canonicalFile
            if (!inside(context, canonical) || !canonical.isFile) return null
            return Uri.Builder().scheme("content").authority("${context.packageName}.exact.files").path(canonical.path).build()
        }

        /** The files another app may write through their URIs now, by canonical path ([writableUri]). */
        private val writable: MutableSet<String> = java.util.Collections.synchronizedSet(HashSet())

        /**
         * A content URI another app may write [f] through, for an intent whose result is a file (the camera app's
         * `EXTRA_OUTPUT`): [f], one of the app's own files, is made empty, and writes to it open until [endWrite].
         * Null for a file outside the app's data directory, or a link.
         */
        fun writableUri(context: Context, f: File): Uri? {
            // Before anything is emptied: a file elsewhere is left as it is.
            if (!inside(context, f.canonicalFile)) return null
            if (!openForWrite(f)) return null
            return uri(context, f) ?: run { endWrite(f); null }
        }

        /** [f] made empty, following no link ([output]), and open to another app's writes through its URI until
         * [endWrite]. */
        internal fun openForWrite(f: File): Boolean {
            try {
                f.parentFile?.mkdirs()
                output(f).close()
            } catch (e: IOException) { return false }
            writable.add(f.canonicalPath)
            return true
        }

        /** Whether another app may write [f] through its URI now. */
        internal fun writes(f: File): Boolean = f.canonicalPath in writable

        /** [f]'s URI opens for reading only again. */
        fun endWrite(f: File) { writable.remove(f.canonicalPath) }

        /** A file's type by its extension, as a receiver reads it. */
        fun type(f: File): String = MimeTypeMap.getSingleton().getMimeTypeFromExtension(f.extension.lowercase()) ?: "application/octet-stream"

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
