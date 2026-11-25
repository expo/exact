// @exact/browser on Android (LLP 1020.002 D4): `clearSiteData` clears a `persist:` partition's AndroidX WebKit
// profile through the reader (`FrameStorage`), which keeps the profiles its frames are shown in.
package exact.browser

import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.FrameStorage
import org.json.JSONArray
import org.json.JSONObject

class BrowserModule(context: ExtensionContext) : ExactExtension(context) {
    override fun later(name: String, args: JSONObject, reply: (JSONObject) -> Unit) {
        if (name != "clearSiteData") return reply(JSONObject().put("error", "NotSupportedError"))
        val partition = args.optString("partition")
        val types = args.opt("types") as? JSONArray
            ?: return reply(JSONObject().put("error", "TypeError").put("message", "clearSiteData: types is a list of type names"))
        val list = (0 until types.length()).map { types.optString(it) }
        FrameStorage.clearSiteData(partition, list) { failure ->
            reply(failure?.let { JSONObject().put("error", it.first).put("message", it.second) }
                ?: JSONObject().put("partition", partition).put("types", types))
        }
    }
}
