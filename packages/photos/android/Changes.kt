// `photoChanges`' token and answer (LLP 1133.019.000 D1), shared by the agent's library and MediaStore's, as Apple's
// `Changes.swift` has them.
package exact.photos

import org.json.JSONArray
import org.json.JSONObject

/** A `photoChanges` token, opaque to the app: base64 of JSON naming the backend that answered it and the view it
 * answered for (the access and the subtypes), then either nothing (a rescan-only view's starting point), a checkpoint
 * ([at]), or a walk's continuation ([at], [end] and the last candidate answered, [after]). The agent's tokens also name
 * its store ([epoch]). */
internal data class ChangeToken(
    val backend: String,
    val access: String,
    val subtypes: List<String>,
    val epoch: String? = null,
    val at: String? = null,
    val end: String? = null,
    val after: String? = null,
) {
    val rescan get() = at == null

    val text: String
        get() {
            val o = JSONObject().put("v", 1).put("backend", backend).put("access", access).put("subtypes", JSONArray(subtypes))
            listOf("epoch" to epoch, "at" to at, "end" to end, "after" to after).forEach { (k, v) -> if (v != null) o.put(k, v) }
            return android.util.Base64.encodeToString(o.toString().toByteArray(), android.util.Base64.NO_WRAP)
        }

    /** Whether [since] was answered for this view. */
    fun sameView(since: ChangeToken) = access == since.access && subtypes == since.subtypes

    /** `photoChanges`' answer. */
    fun answer(added: List<JSONObject> = emptyList(), updated: List<JSONObject> = emptyList(), deleted: List<String> = emptyList(),
               more: Boolean = false, expired: Boolean = false): JSONObject = JSONObject()
        .put("added", JSONArray(added)).put("updated", JSONArray(updated)).put("deleted", JSONArray(deleted))
        .put("token", text).put("more", more).put("expired", expired)

    companion object {
        fun refused(): JSONObject = JSONObject().put("error", "SyntaxError").put("message", "not a token photoChanges answered here")

        /** The view's subtypes: each word once, in order. */
        fun subtypes(args: JSONObject): List<String> =
            args.optJSONArray("subtypes")?.let { a -> (0 until a.length()).map { a.getString(it) } }.orEmpty().toSortedSet().toList()

        /** `photoChanges`' limit: 1 to 500, 500 if none. */
        fun limit(args: JSONObject) = args.optInt("limit", 500).coerceIn(1, 500)

        /** Null for no token or an empty one. Throws [IllegalArgumentException] for text no [backend] answered. */
        fun parse(given: Any?, backend: String): ChangeToken? {
            val text = given as? String
            if (text.isNullOrEmpty()) return null
            // The whole text one JSON object, as Apple's decoder takes it: nothing after it.
            val o = try {
                val tokens = org.json.JSONTokener(String(android.util.Base64.decode(text, android.util.Base64.NO_WRAP)))
                (tokens.nextValue() as? JSONObject)?.takeIf { tokens.nextClean() == 0.toChar() }
            } catch (e: Exception) { null } ?: throw IllegalArgumentException("not a token")
            val words = o.optJSONArray("subtypes")?.let { a -> (0 until a.length()).map { a.opt(it) } }
            require(o.opt("v") == 1 && o.opt("backend") == backend && o.opt("access") is String && words != null && words.all { it is String })
            fun field(k: String): String? {
                if (!o.has(k)) return null
                val v = o.get(k)
                require(v is String && v.isNotEmpty())
                return v
            }
            val t = ChangeToken(backend, o.getString("access"), words!!.map { it as String }, field("epoch"), field("at"),
                field("end"), field("after"))
            // A continuation has both its end and its last key; a rescan-only token has neither. The view is one this
            // module answers, and the text is exactly what its encoder writes for these fields.
            require((t.end == null) == (t.after == null) && !(t.rescan && t.end != null))
            require(t.access in setOf("full", "limited", "none") && t.subtypes == t.subtypes.toSortedSet().toList() && t.subtypes.all { it == "screenshot" })
            require(t.text == text)
            return t
        }
    }
}
