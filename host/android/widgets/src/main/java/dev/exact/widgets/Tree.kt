package dev.exact.widgets

import org.json.JSONArray
import org.json.JSONObject

/**
 * A widget entry laid out by the kernel at one size: the snapshot tree `exact_painted::snapshot::render` answers (LLP
 * 1126.003 D2, `host/painted/src/snapshot/tree.rs`). Every length is in dp; a node's `x` and `y` are in its parent's box.
 */
internal class Node(
    val x: Float,
    val y: Float,
    val width: Float,
    val height: Float,
    val background: Color?,
    val radius: Float,
    val border: Border?,
    val opacity: Float,
    val clip: Boolean,
    /** What of the box's paint the launcher's views cannot draw (`background-image`, `border-radius`, `border`). */
    val picture: List<String>,
    val label: String?,
    val role: String?,
    val heading: Int?,
    val hidden: Boolean,
    val href: String?,
    val lines: List<Line>,
    val time: Time?,
    val progress: Progress?,
    val image: Image?,
    val children: List<Node>,
) {
    companion object {
        fun parse(o: JSONObject): Node = Node(
            x = o.float("x"), y = o.float("y"), width = o.float("w"), height = o.float("h"),
            background = o.optJSONObject("background")?.let(Color::parse),
            radius = o.float("radius"),
            border = o.optJSONObject("border")?.let { Border(it.float("width"), Color.parse(it.getJSONObject("color"))) },
            opacity = if (o.has("opacity")) o.float("opacity") else 1f,
            clip = o.optBoolean("clip"),
            picture = o.optJSONArray("picture").strings(),
            label = o.optString("label").ifEmpty { null },
            role = o.optString("role").ifEmpty { null },
            heading = if (o.has("heading")) o.getInt("heading") else null,
            hidden = o.optBoolean("hidden"),
            href = o.optString("href").ifEmpty { null },
            lines = o.optJSONArray("lines").objects().map(Line::parse),
            time = o.optJSONObject("time")?.let { Time(it.getDouble("datetime"), it.optString("format")) },
            progress = o.optJSONObject("progress")?.let {
                Progress(it.getDouble("value").toFloat(), it.getDouble("max").toFloat(), Color.parse(it.getJSONObject("color")))
            },
            image = o.optJSONObject("image")?.let { Image(it.optString("src"), it.optString("fit")) },
            children = o.optJSONArray("children").objects().map(::parse),
        )
    }
}

/** A colour: a Material role the launcher resolves ([role] is its index, [alpha] its opacity in percent), else ARGB
 * under the light scheme and the dark one. */
internal data class Color(val role: Int?, val alpha: Int, val light: Int, val dark: Int) {
    companion object {
        fun parse(o: JSONObject): Color =
            if (o.has("m")) Color(o.getInt("m"), o.optInt("a", 100), 0, 0)
            else Color(null, 100, o.getLong("l").toInt(), o.getLong("d").toInt())
    }
}

internal data class Border(val width: Float, val color: Color)

/** One kernel line: its text, its box and baseline in the node's box, and its runs. */
internal class Line(
    val text: String,
    val x: Float,
    val top: Float,
    val height: Float,
    val baseline: Float,
    val width: Float,
    val rtl: Boolean,
    val runs: List<Run>,
) {
    companion object {
        fun parse(o: JSONObject) = Line(
            o.getString("text"), o.float("x"), o.float("top"), o.float("height"), o.float("baseline"), o.float("width"),
            o.optBoolean("rtl"), o.optJSONArray("runs").objects().map(Run::parse),
        )
    }
}

/** A run of one style, `[start, end)` in UTF-16 units of the line's text, at `x` and `width` in the node's box. */
internal class Run(
    val start: Int,
    val end: Int,
    val x: Float,
    val width: Float,
    val size: Float,
    val weight: Int,
    val italic: Boolean,
    val tabular: Boolean,
    val letterSpacing: Float,
    val color: Color?,
) {
    companion object {
        fun parse(o: JSONObject) = Run(
            o.getInt("start"), o.getInt("end"), o.float("x"), o.float("width"), o.float("size"),
            o.optInt("weight", 400), o.optBoolean("italic"), o.optBoolean("tabular"), o.float("letterSpacing"),
            o.optJSONObject("color")?.let(Color::parse),
        )
    }
}

/** A `time`'s instant (ms since the epoch) and its format. */
internal data class Time(val datetime: Double, val format: String)

internal data class Progress(val value: Float, val max: Float, val color: Color)

internal data class Image(val src: String, val fit: String)

private fun JSONObject.float(key: String): Float = optDouble(key, 0.0).toFloat()

private fun JSONArray?.objects(): List<JSONObject> =
    if (this == null) emptyList() else List(length()) { getJSONObject(it) }

private fun JSONArray?.strings(): List<String> =
    if (this == null) emptyList() else List(length()) { getString(it) }
