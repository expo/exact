package dev.exact.canvas

import android.content.Context
import java.io.File

/**
 * Where the running generation's assets are, which a sound, a video and a page are opened from by path (LLP 1030.006
 * D1): a signed update's directory, every file checked against its card before it ran ([Native.assetRoot]), else the
 * APK's own, copied into the app's files ([ExactActivity.installAssets]). Asked at each open, so an update that
 * activates is read from then on.
 */
internal object AssetRoot {
    fun of(context: Context): File = Native.assetRoot()?.let(::File) ?: File(context.filesDir, "app")
}
