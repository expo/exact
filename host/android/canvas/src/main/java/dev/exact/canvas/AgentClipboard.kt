package dev.exact.canvas

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context

/** The Android session lives for the process. Contract copy and the package read share its private clipboard. */
object AgentClipboard {
    val active: Boolean get() = agentCarried() && Native.underAgent("clipboard") && android.system.Os.getenv("EXACT_DEVICE") != "real"
    var text: String? = null
        private set

    internal fun copy(context: Context, label: CharSequence?, value: String) {
        if (active) text = value
        else context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(label, value))
    }
}
