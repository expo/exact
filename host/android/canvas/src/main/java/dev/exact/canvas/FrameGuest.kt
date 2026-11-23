package dev.exact.canvas

import org.json.JSONObject

/**
 * An iframe's `partition` (LLP 1020.002 D1): a name, `persist:` and then 1 to 64 of `[A-Za-z0-9._-]`, or the same
 * without `persist:`. A frame this host cannot show as its partition says is refused where the app hears it: it shows
 * `about:blank` and fires `navigateerror`, and is never loaded in the default store.
 */
internal object Partitions {
    private val NAME = Regex("[A-Za-z0-9._-]{1,64}")

    fun valid(name: String) = NAME.matches(name.removePrefix("persist:"))

    /** Why a frame with these props is not shown: a DOMException name and a message, or null. */
    fun refusal(props: JSONObject): Pair<String, String>? {
        val partition = props.partition() ?: return null
        if (!valid(partition)) {
            return "SyntaxError" to "partition \"$partition\" is not a partition name: persist: and 1 to 64 of " +
                "A-Z a-z 0-9 . _ -, or the same without persist:"
        }
        return "NotSupportedError" to "partition \"$partition\": Android does not yet keep an iframe's partition"
    }
}

/** A string prop, empty where the painted host sent `null`. */
internal fun JSONObject.text(key: String): String = if (isNull(key)) "" else optString(key)

/** An iframe's `partition`: null for none, as an empty name is a name, refused. */
internal fun JSONObject.partition(): String? = if (isNull("partition")) null else optString("partition")

/** A `FrameNavigation` record for a frame with no navigation of its own to report (LLP 1020.002 D2). */
internal fun refusalRecord(error: String, message: String, partitioned: Boolean): String = JSONObject()
    .put("url", "").put("destination", "").put("navigationId", 0).put("navigationType", "push")
    .put("title", "").put("icons", org.json.JSONArray()).put("canGoBack", false).put("canGoForward", false)
    .put("loading", false).put("partitioned", partitioned).put("controller", "person")
    .put("error", error).put("message", message).put("recovering", false)
    .toString()
