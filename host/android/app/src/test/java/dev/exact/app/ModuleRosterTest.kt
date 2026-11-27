package dev.exact.app

import dev.exact.canvas.NativeModules
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
// Allow SDK 35 (JDK 17+) and SDK 36 (for apps with minimumOS 16); Gradle pins `robolectric.enabledSdks`
// to `max(35, minSdk)` so each build runs once on the lowest supported SDK.
@Config(minSdk = 35, maxSdk = 36, application = android.app.Application::class)
class ModuleRosterTest {
    @Test fun bakedTagsHaveFactories() {
        val required = System.getProperty("exact.module.tags")?.split(',') ?: return
        val factories = NativeModules.load()
        val missing = required.filter { factories[it] == null }
        if (missing.isEmpty()) return
        val message = "Android: the module artifact lacks factories for " + missing.joinToString(", ")
        if (System.getProperty("exact.module.development") == "true") System.err.println("warning: $message")
        else throw AssertionError(message)
    }
}
