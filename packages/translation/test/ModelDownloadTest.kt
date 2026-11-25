package exact.translation

import android.os.Looper
import com.google.android.gms.tasks.CancellationTokenSource
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.TaskCompletionSource
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.common.model.DownloadConditions
import com.google.mlkit.nl.translate.Translator
import dev.exact.canvas.Native
import dev.exact.canvas.Cause
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implements
import org.robolectric.annotation.Implementation

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class ModelDownloadTest {
    private class Client : Translator {
        val download = TaskCompletionSource<Void>()
        lateinit var conditions: DownloadConditions
        var closed = false
        val calls = ArrayList<String>()
        val cancellations = ArrayList<CancellationTokenSource>()
        val replies = ArrayList<TaskCompletionSource<String>>()
        override fun downloadModelIfNeeded(): Task<Void> = error("missing download conditions")
        override fun downloadModelIfNeeded(conditions: DownloadConditions): Task<Void> {
            this.conditions = conditions
            return download.task
        }
        override fun translate(input: String): Task<String> {
            calls.add(input)
            val cancellation = CancellationTokenSource().also { cancellations.add(it) }
            return TaskCompletionSource<String>(cancellation.token).also { replies.add(it) }.task
        }
        override fun close() { closed = true }
    }

    private val clients = ArrayList<Client>()
    private var eligible = true
    private var online = true
    private var failClient = false
    private var inventory: Task<Set<String>>? = null
    private var installed = emptySet<String>()
    private val module = TranslationModule(ExtensionContext(RuntimeEnvironment.getApplication(), "translation"),
        object : TranslationModels() {
            override fun hasNetwork(context: android.content.Context) = online
            override fun canDownload(context: android.content.Context) = eligible
            override fun downloaded(): Task<Set<String>> = inventory ?: Tasks.forResult(installed)
            override fun client(source: String, target: String) = Client().also {
                check(!failClient) { "client initialization failed" }
                clients.add(it)
            }
        })
    private val pair = JSONObject().put("sourceLanguage", "ja").put("targetLanguage", "en")
    private fun call(name: String, args: JSONObject): List<JSONObject> {
        val replies = ArrayList<JSONObject>()
        module.later(name, args) { replies.add(it) }
        idle()
        return replies
    }
    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    @Test fun everyWaitingCreateHearsTheDownloadFailureAndCanRetry() {
        pair.put("android", JSONObject().put("requireWifi", true).put("requireCharging", true))
        val first = call("createTranslator", pair)
        val second = call("createTranslator", JSONObject(pair.toString()).put("sourceLanguage", "ja-JP"))
        val loader = clients.single()
        assertTrue(loader.conditions.isWifiRequired && loader.conditions.isChargingRequired)
        assertEquals("downloading", call("translatorAvailability", pair).single().getString("availability"))
        loader.download.setException(java.io.IOException("offline"))
        idle()
        for (answer in listOf(first, second)) assertEquals("NetworkError", answer.single().getString("error"))
        assertTrue(loader.closed)
        assertEquals("downloadable", call("translatorAvailability", pair).single().getString("availability"))
        val retry = call("createTranslator", pair)
        val also = call("createTranslator", pair)
        installed = setOf("ja", "en")
        clients.last().download.setResult(null)
        idle()
        for (answer in listOf(retry, also)) assertEquals("google-translate", answer.single().getString("attribution"))
        assertNotEquals(retry.single().getString("id"), also.single().getString("id"))
        module.destroy {}
    }

    @Test fun oneHandleRunsParagraphsAndCallsInOrderAndNeverDownloads() {
        installed = setOf("ja", "en")
        val id = call("createTranslator", pair).single().getString("id")
        fun translate(input: String) = call("translate", JSONObject().put("translator", id).put("input", input))
        val paragraph = "春".repeat(1001)
        val first = translate("$paragraph\n\n\n\n秋")
        val next = translate("冬")
        val client = clients.single()
        assertEquals(listOf(paragraph), client.calls)
        client.replies[0].setResult("spring"); idle()
        assertEquals(listOf(paragraph, "秋"), client.calls)
        client.replies[1].setResult("fall"); idle()
        assertEquals("spring\n\n\n\nfall", first.single().getString("text"))
        assertEquals(listOf(paragraph, "秋", "冬"), client.calls)
        client.replies[2].setResult("winter"); idle()
        assertEquals("winter", next.single().getString("text"))
        installed = emptySet()
        val missing = translate("雪").single()
        assertEquals("InvalidStateError", missing.getString("error"))
        assertEquals("not-installed", missing.getString("message"))
        assertEquals(1, clients.size)
        assertTrue(client.closed)
        module.destroy {}
    }

    @Test fun offlineCreationFailsForAMissingModelAndUsesAnInstalledOne() {
        online = false
        assertEquals("NetworkError", call("createTranslator", pair).single().getString("error"))
        assertTrue(clients.isEmpty())
        assertEquals("downloadable", call("translatorAvailability", pair).single().getString("availability"))
        installed = setOf("en", "ja")
        assertTrue(call("createTranslator", pair).single().has("id"))
        module.destroy {}
    }

    @Test fun aFailedInventoryReadCannotPromiseAModel() {
        inventory = Tasks.forException(java.io.IOException("inventory unavailable"))
        assertEquals("model-not-ready", call("translatorAvailability", pair).single().getString("reason"))
        val answer = call("createTranslator", pair).single()
        assertEquals("OperationError", answer.getString("error"))
        assertEquals("model-not-ready", answer.getString("message"))
        assertTrue(clients.isEmpty())
    }

    @Test fun expectedLanguagesMustBeIdentifiableByTheBundledDetector() {
        fun expected(vararg tags: String) = JSONObject().put("expectedInputLanguages", org.json.JSONArray(tags.toList()))
        assertEquals("unsupported-options", call("languageDetectorAvailability", expected("ja", "xx")).single().getString("reason"))
        assertEquals("NotSupportedError", call("createLanguageDetector", expected("xx")).single().getString("error"))
        val known = expected("ar-Latn", "haw", "zh-TW")
        assertEquals("available", call("languageDetectorAvailability", known).single().getString("availability"))
        assertEquals(known.getJSONArray("expectedInputLanguages").toString(),
            call("createLanguageDetector", known).single().getJSONArray("expectedInputLanguages").toString())
    }

    @Test fun withoutPlayServicesOnlyInstalledPairsCanBeCreated() {
        eligible = false
        assertEquals("device-not-eligible", call("translatorAvailability", pair).single().getString("reason"))
        assertEquals("NotSupportedError", call("createTranslator", pair).single().getString("error"))
        installed = setOf("ja", "en")
        assertEquals("available", call("translatorAvailability", pair).single().getString("availability"))
        assertTrue(call("createTranslator", pair).single().has("id"))
        assertTrue(clients.isEmpty())
        module.destroy {}
    }

    @Test fun cancelledTasksAndDestroyedHandlesSettleTheirQueuesOnce() {
        installed = setOf("ja", "en")
        val id = call("createTranslator", pair).single().getString("id")
        val args = JSONObject().put("translator", id).put("input", "雪")
        failClient = true
        assertEquals("OperationError", call("translate", args).single().getString("error"))
        failClient = false
        val first = call("translate", args)
        val second = call("translate", args)
        val third = call("translate", args)
        val client = clients.single()
        client.cancellations.first().cancel()
        idle()
        assertEquals("AbortError", first.single().getString("error"))
        assertEquals(2, client.calls.size)
        call("destroyTranslator", args)
        client.replies.last().setResult("late")
        idle()
        for (reply in listOf(second, third)) assertEquals("AbortError", reply.single().getString("error"))
        assertEquals(2, client.calls.size)
        module.destroy {}
    }

    @Test fun aCancelledCreateCannotStartADownloadOrSettleANewerRequest() {
        val firstRead = TaskCompletionSource<Set<String>>()
        inventory = firstRead.task
        val first = ArrayList<JSONObject>()
        module.later("createTranslator", pair, null, Cause("app", request = 1)) { first.add(it) }
        module.cancel(1)
        val secondRead = TaskCompletionSource<Set<String>>()
        inventory = secondRead.task
        val second = call("createTranslator", pair)
        val joining = ArrayList<JSONObject>()
        module.later("createTranslator", pair, null, Cause("app", request = 2)) { joining.add(it) }
        module.cancel(2)
        firstRead.setResult(emptySet()); idle()
        assertTrue(clients.isEmpty())
        assertTrue(second.isEmpty())
        secondRead.setResult(emptySet()); idle()
        clients.single().download.setResult(null); idle()
        assertTrue(second.single().has("id"))
        for (reply in listOf(first, joining)) assertEquals("AbortError", reply.single().getString("error"))
        module.destroy {}
    }

    @Implements(Native::class, isInAndroidSdk = false)
    class AgentHost {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun underAgent(name: String) = true
        }
    }

    @Test @Config(shadows = [AgentHost::class])
    fun simultaneousHandlesTakeScriptEntriesInAdmissionOrder() {
        val a = call("createTranslator", pair).single().getString("id")
        val b = call("createTranslator", pair).single().getString("id")
        module.preferred(JSONObject("""{"translations":[{"text":"one","error":null},{"text":"two"},{"text":"three"}]}"""))
        val replies = listOf(a, a, b).map { id ->
            ArrayList<JSONObject>().also { result ->
                module.later("translate", JSONObject().put("translator", id).put("input", "雪")) { result.add(it) }
            }
        }
        idle()
        assertEquals(listOf("one", "two", "three"), replies.map { it.single().getString("text") })
        module.destroy {}
    }

    @Test fun anEndedSessionClosesTheLoaderAndCannotDeliverALateHandle() {
        val first = call("createTranslator", pair)
        val second = call("createTranslator", pair)
        module.destroy {}
        val loader = clients.single()
        assertTrue(loader.closed)
        loader.download.setResult(null)
        idle()
        for (answer in listOf(first, second)) assertEquals("AbortError", answer.single().getString("error"))
    }
}
