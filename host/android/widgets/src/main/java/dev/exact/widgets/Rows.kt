package dev.exact.widgets

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/**
 * The app's widget rows as its build wrote them (`res/raw/exact_widgets.json`, app/build.gradle.kts): each Android
 * widget's kind, title, description, preview props and configuration, from app.json's `widgets`.
 */
internal class Row(val kind: String, val title: String, val description: String, val preview: String, val parameters: List<Parameter>) {
    class Parameter(val name: String, val title: String, val default: String, val values: List<Pair<String, String>>)

    /** The configuration a placement uses with no choices of its own: every parameter's default. */
    fun defaults(): JSONObject = JSONObject().also { o -> parameters.forEach { o.put(it.name, it.default) } }

    companion object {
        fun all(context: Context): List<Row> {
            val id = context.resources.getIdentifier("exact_widgets", "raw", context.packageName)
            if (id == 0) return emptyList()
            val text = context.resources.openRawResource(id).use { it.readBytes().decodeToString() }
            val rows = JSONArray(text)
            return List(rows.length()) { parse(rows.getJSONObject(it)) }
        }

        fun of(context: Context, kind: String): Row? = all(context).firstOrNull { it.kind == kind }

        fun parse(o: JSONObject): Row {
            val parameters = o.optJSONObject("configuration")?.optJSONArray("parameters")
            return Row(
                o.getString("kind"), o.optString("title", o.getString("kind")), o.optString("description"),
                o.optJSONObject("preview")?.toString() ?: "{}",
                if (parameters == null) emptyList() else List(parameters.length()) { i ->
                    val p = parameters.getJSONObject(i)
                    val values = p.getJSONArray("values")
                    Parameter(p.getString("name"), p.optString("title", p.getString("name")), p.getString("default"),
                        List(values.length()) { j -> values.getJSONObject(j).let { it.getString("value") to it.optString("title", it.getString("value")) } })
                },
            )
        }
    }
}
