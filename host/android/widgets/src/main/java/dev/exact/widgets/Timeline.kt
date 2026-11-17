package dev.exact.widgets

import org.json.JSONArray
import org.json.JSONObject

/**
 * A refresh's timeline, kept as the last good copy between refreshes (LLP 1126.003 D5): each entry's date (ms since the
 * epoch) and props (JSON), dates strictly increasing.
 */
internal class Timeline(val entries: List<Entry>) {
    class Entry(val date: Double, val props: String)

    /** The entry shown at [now]: the latest not after it, else the first (a timeline that starts in the future). */
    fun current(now: Double): Entry = entries.lastOrNull { it.date <= now } ?: entries.first()

    /** When the entry shown changes next, or null after the last. */
    fun next(now: Double): Double? = entries.firstOrNull { it.date > now }?.date

    /** When the next refresh is due after a good one at [now]: past the last entry, and at least 15 minutes on, as
     * WidgetKit's `.after(max(now + 15 min, last + 1 s))` (LLP 1126 D3). */
    fun refreshDue(now: Double): Double = maxOf(now + RETRY, entries.last().date + 1_000.0)

    fun json(): String = JSONArray(entries.map { JSONObject().put("date", it.date).put("props", JSONObject(it.props)) }).toString()

    companion object {
        /** A failed refresh is tried again this much later (ms), as on iOS. */
        const val RETRY = 15 * 60_000.0

        /** A refresh's answer, admitted as Apple admits it (`Timeline.swift`): 1 to 100 entries within 1 MiB, dates
         * finite and strictly increasing. Throws what it refuses. */
        fun admit(json: String): Timeline {
            if (json.length > 1 shl 20) throw IllegalArgumentException("a timeline over 1 MiB")
            val parsed = runCatching { JSONArray(json) }.getOrElse {
                throw IllegalArgumentException(runCatching { JSONObject(json).getString("error") }.getOrDefault("not a timeline"))
            }
            if (parsed.length() !in 1..100) throw IllegalArgumentException("refresh must return 1 to 100 entries")
            val entries = try {
                List(parsed.length()) {
                    val o = parsed.getJSONObject(it)
                    Entry(o.getDouble("date"), o.getJSONObject("props").toString())
                }
            } catch (e: org.json.JSONException) {
                throw IllegalArgumentException("a timeline entry is not a date and props: ${e.message}")
            }
            for ((i, e) in entries.withIndex()) {
                if (!e.date.isFinite() || (i > 0 && e.date <= entries[i - 1].date)) {
                    throw IllegalArgumentException("timeline dates must be finite and strictly increasing")
                }
            }
            return Timeline(entries)
        }
    }
}
