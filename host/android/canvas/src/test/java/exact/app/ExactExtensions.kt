package exact.app

import dev.exact.canvas.ExactExtension
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionRoster

/** The roster the reader finds by this name, as an app's build writes it. It binds nothing unless a test fills it. */
class ExactExtensions : ExtensionRoster {
    override val extensions get() = bound
    override val startAtLoad get() = atLoad

    companion object {
        val bound = LinkedHashMap<String, (ExtensionContext) -> ExactExtension>()
        var atLoad = emptyList<String>()

        /** A process whose host is not yet set up and has made no extension, with [extensions] bound. */
        fun process(extensions: Map<String, (ExtensionContext) -> ExactExtension> = emptyMap(), atLoad: List<String> = emptyList()) {
            bound.clear()
            bound += extensions
            this.atLoad = atLoad
            val reader = Class.forName("dev.exact.canvas.Extensions")
            fun field(name: String) = reader.getDeclaredField(name).apply { isAccessible = true }
            (field("made").get(null) as MutableMap<*, *>).clear()
            field("application").set(null, null)
        }
    }
}
