package dev.exact.canvas

import java.io.File
import java.io.IOException

/** What an `iframe`'s or a `video`'s `src` names ([Sources]). */
internal sealed class Source {
    /** A URL the view loads as given. */
    data class Remote(val url: String) : Source()
    /** A file under the asset root or the app's own, with the query and fragment the `src` gave. */
    data class Local(val file: File, val suffix: String = "") : Source()
    /** A `src` that names nothing the element may show, and why. */
    data class Refused(val why: String) : Source()
}

/**
 * The resolution Apple's hosts give these sources: a scheme-less `src` is a file under the asset root and never one
 * outside it (`WebArm.localBytes`, `host/apple/webarm/FrameFiles.swift`); a `video` also plays `http(s)` and `app:/`
 * (`VideoModule`, `NodeView.resolveSource`).
 */
internal object Sources {
    /** The schemes a frame loads as a URL (`WebArm.remoteSource`). */
    private val frameSchemes = setOf("http", "https", "data", "about", "blob")

    /** An `iframe`'s `src`. A protocol-relative one is `https:`, as `WebModule.browserSource` makes it. */
    fun frame(src: String, assets: File): Source {
        if (src.isEmpty()) return Source.Remote("about:blank")
        val source = if (src.startsWith("//")) "https:$src" else src
        if (scheme(source) in frameSchemes) return Source.Remote(source)
        return asset(source, assets)
    }

    /**
     * The headers a frame's remote page is asked for with. A browser asks for a cross-origin frame's page with the
     * parent's origin as its `Referer` (`strict-origin-when-cross-origin`, the default policy), and an embed that
     * requires one refuses the load without it (YouTube's Error 153). The frame's WebView is a page of its own with no
     * parent, so it is given the origin Apple's frames send, the host's frame page at `exact.invalid` in the guest's
     * scheme (`GuestURL.request`, `host/apple/webarm/FrameBrowser.swift`). WebView makes a `Referer` passed here the
     * navigation's referrer. The map is mutable because some older WebViews require it (`WebView.loadUrl`).
     */
    fun frameHeaders(url: String): MutableMap<String, String> = when (val scheme = scheme(url)) {
        "http", "https" -> mutableMapOf("Referer" to "$scheme://exact.invalid/")
        else -> mutableMapOf()
    }

    /** A `video`'s `src`; [appFile] is the file an `app:/data|cache|tmp/…` path names, or null (`Native.appFile`). */
    fun video(src: String, assets: File, appFile: (String) -> File?): Source = when {
        src.isEmpty() -> Source.Refused("Empty src attribute")
        src.startsWith("app:/") -> appFile(src)?.let { Source.Local(it) } ?: Source.Refused("$src is not an app file")
        else -> when (scheme(src)) {
            null -> asset(src, assets).let { if (it is Source.Local) Source.Local(it.file) else it }
            "http", "https" -> Source.Remote(src)
            else -> Source.Refused("Unsupported media source")
        }
    }

    /** Whether a guest's navigation to [scheme] and [path] stays in its frame: a web URL, or a file under [assets]. */
    fun navigable(scheme: String?, path: String?, assets: File): Boolean = when (scheme?.lowercase()) {
        "http", "https", "data", "about", "blob" -> true
        "file" -> path != null && under(File(path), assets)
        else -> false
    }

    /** The page a refused frame shows, as Apple's `WebArm.errorDocument`. */
    fun errorPage(message: String): String {
        val text = message.replace("&", "&amp;").replace("\"", "&quot;").replace("<", "&lt;").replace(">", "&gt;")
        return "<!doctype html><meta charset=utf-8><style>body{font:14px system-ui;padding:16px;color:#6b1d1d;" +
            "background:#fff3f3}</style><p>$text</p>"
    }

    /** The scheme of an absolute URL (RFC 3986 §3.1), lowercased, or null for a path. */
    private fun scheme(src: String): String? =
        Regex("^([A-Za-z][A-Za-z0-9+.-]*):").find(src)?.groupValues?.get(1)?.lowercase()

    /** A path under [assets], its leading slashes dropped and its query and fragment kept apart. */
    private fun asset(src: String, assets: File): Source {
        val end = src.indexOfAny(charArrayOf('?', '#')).let { if (it < 0) src.length else it }
        val file = File(assets, src.substring(0, end).trimStart('/'))
        return if (under(file, assets) && file.isFile) Source.Local(file.canonicalFile, src.substring(end))
        else Source.Refused("$src is not a file under ${assets.path}")
    }

    /** Whether [file], its `..` and symbolic links resolved, is inside [root]. */
    private fun under(file: File, root: File): Boolean = try {
        val base = root.canonicalPath
        file.canonicalPath.startsWith(base.trimEnd(File.separatorChar) + File.separator)
    } catch (e: IOException) {
        false
    }
}
