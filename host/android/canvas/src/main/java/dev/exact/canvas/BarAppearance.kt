package dev.exact.canvas

/** Android's LIGHT appearance flags request dark icons, independently for each bar (LLP 1105 D7); [hidden] is
 * `system-bars-hidden`, resolved as the style is. */
internal data class BarAppearance(val statusLight: Boolean, val navigationLight: Boolean, val hidden: Boolean = false)

/** Chrome word 5: bit 0 is the dark scheme; bit 1 requests light status bar icons; bit 2 hides the bars. */
internal fun barAppearance(flags: Int) = BarAppearance(
    statusLight = flags and 2 == 0,
    navigationLight = flags and 1 == 0,
    hidden = flags and 4 != 0,
)
