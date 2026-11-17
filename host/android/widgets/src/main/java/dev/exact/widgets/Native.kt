package dev.exact.widgets

/**
 * The app's library, as the widgets call it (`exact_painted::widgets_jni!`, LLP 1126.003 D9): the same
 * `libexactcanvas.so` the reader loads, here in the `:widgets` process. Every call is made on [Widgets]' one worker
 * thread and answers JSON, or `{"error": …}`.
 */
internal object Native {
    init { System.loadLibrary("exactcanvas") }

    /** Render [kind] at each (width, height) pair of [sizes] in dp, at [now] (ms since the epoch), with [fontScale]
     * the device's font scale; [props] and [configuration] are JSON, null for the preview and the defaults. */
    @JvmStatic external fun render(dir: ByteArray, kind: ByteArray, props: ByteArray?, configuration: ByteArray?, sizes: FloatArray, now: Double, fontScale: Float): ByteArray

    /** Run [kind]'s refresh at [now] within [budget] ms: its timeline. */
    @JvmStatic external fun timeline(dir: ByteArray, kind: ByteArray, now: Double, budget: Long): ByteArray
}
