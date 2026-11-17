package exact.mediacapture

import android.util.Range
import org.json.JSONObject
import kotlin.math.abs

internal object FrameRates {
    // Constraints are bare ideals (LLP 1133.008 D1); dictionaries are not admitted.
    fun requested(value: Any?): Double? = (value as? Number)?.toDouble()?.takeIf { it.isFinite() && it > 0 }

    // CameraX's AeFpsRangeLegacyQuirk corrects each endpoint reported in thousandths.
    private fun normalize(ranges: Collection<Range<Int>>): List<Range<Int>> = ranges.mapNotNull {
        val lower = if (it.lower >= 1000) it.lower / 1000 else it.lower
        val upper = if (it.upper >= 1000) it.upper / 1000 else it.upper
        if (lower > 0 && lower <= upper) Range(lower, upper) else null
    }

    // Media Capture fitness distance: https://www.w3.org/TR/mediacapture-streams/#dfn-fitness-distance
    // The relative distance selects 60 over 30 for an ideal of 44.
    fun choose(ranges: Collection<Range<Int>>, ideal: Double): Range<Int>? = normalize(ranges)
        .minWithOrNull(compareBy<Range<Int>> { abs(it.upper - ideal) / maxOf(it.upper.toDouble(), ideal) }
            .thenBy { it.upper - it.lower }.thenBy { it.upper })

    fun capability(ranges: Collection<Range<Int>>): JSONObject? = normalize(ranges).takeIf { it.isNotEmpty() }?.let {
        JSONObject().put("min", it.minOf { r -> r.lower }).put("max", it.maxOf { r -> r.upper })
    }
}
