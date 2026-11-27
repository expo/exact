package dev.exact.canvas

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebView
import androidx.webkit.ScriptHandler
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject

/** Live allocation snapshot for an unpartitioned frame's message bridge ([FrameBridge]). */
internal data class FrameMessageTarget(
    val id: Int,
    val slot: Int,
    val generation: Int,
    val session: Long,
    val binding: Int,
    val src: String,
    val partitioned: Boolean,
    val revoked: Boolean,
    val send: (String) -> Unit,
)

/** Live allocation snapshot for Android-only grant-gated `WebChromeClient.onPermissionRequest`. */
internal data class FrameMediaGrantTarget(
    val context: Context,
    val src: String,
    val partitioned: Boolean,
    val revoked: Boolean,
    val bootMediaGrants: Int,
)

/**
 * Unpartitioned `iframe` `postMessage` bridge and Android-only grant-gated `WebChromeClient` (LLP 1156.008.001 B1).
 * Never attached to a partitioned frame (keyed strictly on the `partitioned` boolean, never `profile != null`).
 */
internal class FrameBridge(private val view: WebView, private val host: FrameHost) {
    private val hasDocumentStart = WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)
    private var startScript: ScriptHandler? = null
    private var activeNonce: String? = null
    private var activeBinding: Int = -1
    private val pendingReplies = ArrayDeque<String>()

    init {
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(view, CHANNEL, setOf("*")) { web, message, sourceOrigin, isMainFrame, _ ->
                if (!isMainFrame || message.type != WebMessageCompat.TYPE_STRING) return@addWebMessageListener
                val raw = message.data ?: return@addWebMessageListener
                onChannelMessage(web, sourceOrigin, raw)
            }
        }
    }

    fun prepareForLoad() {
        resetDocument()
        if (hasDocumentStart && startScript == null) {
            startScript = WebViewCompat.addDocumentStartJavaScript(view, SCRIPT, setOf("*"))
        }
    }

    fun onDocumentCommitted() {
        resetDocument()
    }

    fun ensureInjected() {
        if (!hasDocumentStart && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            view.evaluateJavascript(SCRIPT, null)
        }
    }

    fun reset() {
        resetDocument()
        startScript?.remove()
        startScript = null
    }

    fun postMessage(text: String) {
        val target = host.messageTarget(view) ?: return
        if (target.partitioned || target.revoked) return
        val nonce = activeNonce
        if (nonce != null && activeBinding == target.binding) {
            deliver(text, nonce)
        } else {
            if (pendingReplies.size >= MAX_PENDING_REPLIES) pendingReplies.removeFirst()
            pendingReplies.addLast(text)
        }
    }

    private fun resetDocument() {
        activeNonce = null
        activeBinding = -1
        pendingReplies.clear()
    }

    private fun deliver(text: String, nonce: String) {
        view.evaluateJavascript(
            "globalThis.__exactHostReply?.(${JSONObject.quote(text)}, ${JSONObject.quote(nonce)})",
            null,
        )
    }

    private fun onChannelMessage(web: WebView, sourceOrigin: Uri, raw: String) {
        val target = host.messageTarget(web) ?: return
        if (target.partitioned || target.revoked) return
        if (!messageOriginMatches(sourceOrigin, target.src, host.assets)) return
        if (raw.startsWith("d:")) {
            val nonce = raw.substring(2)
            if (nonce.isEmpty()) return
            activeNonce = nonce
            activeBinding = target.binding
            while (pendingReplies.isNotEmpty()) deliver(pendingReplies.removeFirst(), nonce)
            return
        }
        if (raw.startsWith("m:")) {
            val rest = raw.substring(2)
            val sep = rest.indexOf(':')
            if (sep <= 0) return
            val nonce = rest.substring(0, sep)
            val payload = rest.substring(sep + 1)
            if (nonce != activeNonce || target.binding != activeBinding) return
            target.send(payload)
        }
    }

    companion object {
        const val CHANNEL = "__exactFrame"
        const val GRANT_CAMERA = 1
        const val GRANT_MICROPHONE = 2
        private const val MAX_PENDING_REPLIES = 64

        /** Matches `WebArm.swift`'s direct guest bridge so authored pages work identically on Apple and Android. */
        private val SCRIPT = """
            (() => {
                if (window !== window.top || globalThis.__exactHostReply) return;
                const bridge = window.$CHANNEL;
                if (!bridge) return;
                const replies = new WeakSet();
                const nonce = Array.from(crypto.getRandomValues(new Uint32Array(4)), n => n.toString(16)).join('-');
                globalThis.__exactHostReply = (text, expected) => {
                    if (expected !== nonce) return;
                    const e = new MessageEvent('message', {data: text, source: window});
                    replies.add(e);
                    window.dispatchEvent(e);
                };
                addEventListener('message', e => {
                    if (replies.has(e) || e.source !== window) return;
                    e.stopImmediatePropagation();
                    let p = e.data;
                    if (typeof p !== 'string') {
                        try { p = JSON.stringify(p); } catch { return; }
                    }
                    if (typeof p === 'string') bridge.postMessage('m:' + nonce + ':' + p);
                }, true);
                bridge.postMessage('d:' + nonce);
            })()
        """.trimIndent()

        /**
         * Chrome client for an iframe WebView:
         * - Partitioned frames preserve `onReceivedTitle` and unconditionally deny `onPermissionRequest`.
         * - Unpartitioned frames grant camera/microphone only when immutable boot grants, Android runtime
         *   permissions, and exact `request.origin == authored src origin` all hold; every other request is denied.
         */
        fun chromeClient(web: WebView, host: FrameHost, partitioned: Boolean): WebChromeClient = if (partitioned) {
            object : WebChromeClient() {
                override fun onReceivedTitle(view: WebView, title: String?) {
                    host.browser(view)?.stateChanged()
                }
                override fun onPermissionRequest(request: PermissionRequest) {
                    request.deny()
                }
            }
        } else {
            object : WebChromeClient() {
                override fun onPermissionRequest(request: PermissionRequest) {
                    val target = host.mediaGrantTarget(web)
                    if (target == null || target.partitioned || target.revoked ||
                        !mediaOriginMatches(request.origin, target.src)
                    ) {
                        request.deny()
                        return
                    }
                    val allowed = ArrayList<String>()
                    for (resource in request.resources) {
                        when (resource) {
                            PermissionRequest.RESOURCE_VIDEO_CAPTURE -> {
                                if ((target.bootMediaGrants and GRANT_CAMERA) != 0 &&
                                    target.context.checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
                                ) allowed.add(resource)
                            }
                            PermissionRequest.RESOURCE_AUDIO_CAPTURE -> {
                                if ((target.bootMediaGrants and GRANT_MICROPHONE) != 0 &&
                                    target.context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                                ) allowed.add(resource)
                            }
                        }
                    }
                    if (allowed.isNotEmpty() && allowed.size == request.resources.size) {
                        request.grant(allowed.toTypedArray())
                    } else {
                        request.deny()
                    }
                }
            }
        }

        internal fun normalizedHttpOrigin(uri: Uri?): Triple<String, String, Int>? {
            val scheme = uri?.scheme?.lowercase() ?: return null
            if (scheme != "http" && scheme != "https") return null
            val host = uri.host?.lowercase()?.takeIf { it.isNotEmpty() } ?: return null
            val port = if (uri.port >= 0) uri.port else if (scheme == "https") 443 else 80
            return Triple(scheme, host, port)
        }

        /** Media capture requires an explicit matching `http(s)` origin (never `file://` or opaque). */
        internal fun mediaOriginMatches(requestOrigin: Uri?, authoredSrc: String): Boolean {
            val expected = normalizedHttpOrigin(Uri.parse(authoredSrc)) ?: return false
            val actual = normalizedHttpOrigin(requestOrigin) ?: return false
            return expected == actual
        }

        /** Guest `postMessage` accepts exact `http(s)` origin match, or opaque `file://` origin for bundled local assets. */
        internal fun messageOriginMatches(sourceOrigin: Uri?, authoredSrc: String, assets: java.io.File): Boolean {
            return when (val source = Sources.frame(authoredSrc, assets)) {
                is Source.Remote -> normalizedHttpOrigin(Uri.parse(source.url))?.let { it == normalizedHttpOrigin(sourceOrigin) } == true
                is Source.Local -> {
                    val s = sourceOrigin?.toString().orEmpty()
                    s == "null" || sourceOrigin?.scheme.equals("file", ignoreCase = true)
                }
                is Source.Refused -> false
            }
        }
    }
}
