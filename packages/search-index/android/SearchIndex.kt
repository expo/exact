// @exact/search-index on Android (LLP 1133.023 D4, §9): the mirror only. Android documents no system search that a
// third-party app's content reaches (AppSearch's platform storage names no surface), so the entries are kept in the
// app's files, where they answer `contentIndexEntries` and will resolve AppFunctions' entity parameters (D9). The file
// and its limits are the Apple host's (`ContentIndex.swift`): an entry's identity is its kind and id, at most 5,000
// entries, and at most 1,024 bytes of title, description and URL each; a change past them refuses whole.
package exact.searchindex

import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

class SearchIndexModule(context: ExtensionContext) : ExactExtension(context) {
    private companion object {
        const val MOST = 5_000
        const val TEXT_BYTES = 1_024
    }

    private val file get() = File(context.application.filesDir, "exact-content-index.json")

    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        val refused = when (name) {
            "setContentIndex" -> set(args.optString("kind").takeIf { args.has("kind") }, args.optJSONArray("entries") ?: JSONArray(), args.opt("within"))
            "addContentIndexEntry" -> add(args.optJSONObject("description") ?: JSONObject(), args.opt("within"))
            "deleteContentIndexEntry" -> { delete(args.optString("id")); null }
            "contentIndexEntries" -> return reply(JSONObject().put("exact:answer", JSONArray(load().map { JSONObject(it.toString()).apply { remove("within") } })))
            else -> return reply(JSONObject().put("error", "NotSupportedError"))
        }
        if (refused != null) return reply(JSONObject().put("error", refused))
        context.changed("search-index.contentIndexEntries")
        reply(JSONObject())
    }

    private fun key(e: JSONObject) = e.optString("kind") + ":" + e.optString("id")

    private fun load(): List<JSONObject> = try {
        val entries = JSONObject(file.readText()).getJSONArray("entries")
        (0 until entries.length()).map { entries.getJSONObject(it) }
    } catch (_: Exception) { emptyList() }

    private fun save(entries: List<JSONObject>) {
        val next = File(file.path + ".new")
        next.writeText(JSONObject().put("entries", JSONArray(entries)).toString())
        next.renameTo(file)
    }

    private fun refusal(entries: List<JSONObject>): String? {
        if (entries.size > MOST) return "QuotaExceededError"
        for (e in entries) {
            val url = e.optString("url")
            if (e.optString("id").isEmpty() || e.optString("title").isEmpty() || !url.startsWith("/") || url.startsWith("//")) return "TypeError"
            if (e.has("within") && !e.isNull("within")) {
                val parents = e.optJSONArray("within") ?: return "TypeError"
                if ((0 until parents.length()).any { parents.optString(it).isEmpty() || parents.get(it) !is String }) return "TypeError"
            }
            val text = listOf("title", "description", "url").joinToString("") { e.optString(it) }
            if (text.toByteArray(Charsets.UTF_8).size > TEXT_BYTES) return "QuotaExceededError"
        }
        return null
    }

    /** The entries of `kind` replaced by `entries`, in their order; the other kinds kept. */
    private fun set(kind: String?, entries: JSONArray, within: Any?): String? {
        val copies = (0 until entries.length()).map { i -> JSONObject(entries.getJSONObject(i).toString()).also { if (kind != null) it.put("kind", kind) } }
        val tagged = placed(copies, within) ?: return "TypeError"
        val all = load().filter { it.optString("kind") != (kind ?: "") } + tagged
        return refusal(all) ?: run { save(all); null }
    }

    private fun placed(entries: List<JSONObject>, within: Any?): List<JSONObject>? {
        if (within == null || within == JSONObject.NULL) return entries
        val rows = within as? JSONArray ?: return null
        for (i in 0 until rows.length()) {
            val row = rows.optJSONObject(i) ?: return null
            val parents = row.optJSONArray("within") ?: return null
            val entry = entries.firstOrNull { it.optString("id") == row.optString("id") } ?: return null
            entry.put("within", parents)
        }
        return entries
    }

    private fun add(description: JSONObject, within: Any?): String? {
        val entry = JSONObject(description.toString())
        if (within != null && within != JSONObject.NULL) entry.put("within", within)
        val all = load().filter { key(it) != key(entry) } + entry
        return refusal(all) ?: run { save(all); null }
    }

    private fun delete(id: String) {
        val was = load()
        val now = was.filter { it.optString("id") != id }
        if (now.size != was.size) save(now)
    }
}
