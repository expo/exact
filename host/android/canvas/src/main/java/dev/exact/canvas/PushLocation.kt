package dev.exact.canvas

import org.json.JSONException
import org.json.JSONObject

/** The push tap's location (LLP 1133.004 §9 P5): Expo carries the sender's data as JSON text in FCM's `body`.
 * The top-level key wins, and another sender's body is not read. The lookup reads FCM data or a tap's intent extras. */
object PushLocation {
    fun of(data: (String) -> String?): String? {
        data("navigate")?.let { return it.takeIf { text -> text.isNotBlank() } }
        if (data("experienceId") == null && data("scopeKey") == null) return null
        val body = data("body") ?: return null
        return try {
            (JSONObject(body).opt("navigate") as? String)?.takeIf { it.isNotBlank() }
        } catch (_: JSONException) { null }
    }
}
