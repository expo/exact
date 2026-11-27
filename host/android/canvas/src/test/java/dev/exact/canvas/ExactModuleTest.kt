package dev.exact.canvas

import android.app.Activity
import android.os.Looper
import exact.app.Module
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [ExactModuleTest.Host::class])
class ExactModuleTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            internal val handed = ArrayList<OwnerReplies.Handed>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun now() = 123.0
            @JvmStatic @Implementation fun answer(id: Int, value: String?) { handed += OwnerReplies.Handed.Answer(id, value) }
            @JvmStatic @Implementation fun refuseModule(id: Int, error: String, message: String?) { handed += OwnerReplies.Handed.Refused(id, error, message) }
            @JvmStatic @Implementation fun changed(topic: String) { handed += OwnerReplies.Handed.Changed(topic) }
            @JvmStatic @Implementation fun moduleAfter(timer: Int, due: Double) { handed += OwnerReplies.Handed.After(timer, due) }
        }
    }

    @Before fun clearDeliveries() { Host.handed.clear() }

    @Test fun identityAdmissionNeedsNoTurnOfMainInEitherKindOfSession() {
        val body = """{"extension":"sign-in","sync":"getIdentityCapabilitiesSync","args":{}}"""
        val answer = """{"providers":{"google":{"active":true}}}"""
        val reader = SynchronousExtension({ _, request ->
            assertNotSame(Looper.getMainLooper(), Looper.myLooper())
            if (request.getString("sync") == "getIdentityCapabilitiesSync" && !request.has("owner")) JSONObject(answer) else null
        }) { error("a read cannot construct a UI extension") }
        exact.app.ExactExtensions.process(mapOf("sign-in" to reader))
        val app = RuntimeEnvironment.getApplication() as android.app.Application
        Extensions.bind(app)
        val presenter = android.os.HandlerThread("identity admission").apply { start() }
        try {
            val owner = android.os.Handler(presenter.looper)
            fun read(call: () -> String): String {
                val task = java.util.concurrent.FutureTask(call)
                owner.post(task)
                return task.get(5, java.util.concurrent.TimeUnit.SECONDS)
            }
            assertEquals("=$answer", read { ModuleSession.sync(body) })
            val session = checkNotNull(ModuleSession.open(app))
            try {
                assertEquals("=$answer", read { session.call(body) })
            } finally { session.end() }
            val refused = JSONObject(read { ModuleSession.sync("""{"extension":"sign-in","sync":"getIdentityCapabilitiesSync","owner":"other"}""") }.removePrefix("="))
            assertEquals("NotSupportedError", refused.getString("error"))
            assertFalse(refused.has("message"))
        } finally {
            presenter.quitSafely()
            presenter.join(5000)
            exact.app.ExactExtensions.process()
        }
    }

    @Test fun aModuleThatCannotBeConstructedDoesNotBecomeAvailable() {
        Module.broken = true
        try {
            assertNull(ModuleSession.open(RuntimeEnvironment.getApplication()))
            assertNull(ModuleSession.current)
        } finally { Module.broken = false }
    }

    @Test fun constructionCanUseTheClockAndPublishesOnlyOnSuccess() {
        Module.armInConstructor = true
        try {
            Module.broken = true
            assertNull(ModuleSession.open(RuntimeEnvironment.getApplication()))
            Module.broken = false
            val session = checkNotNull(ModuleSession.open(RuntimeEnvironment.getApplication()))
            try {
                shadowOf(Looper.getMainLooper()).idle()
                val timer = Host.handed.single() as OwnerReplies.Handed.After
                assertEquals(128.0, timer.due, 0.0)
                session.timer(timer.timer)
                shadowOf(Looper.getMainLooper()).idle()
                assertEquals(OwnerReplies.Handed.Changed("built"), Host.handed.last())
            } finally { session.end() }
        } finally { Module.broken = false; Module.armInConstructor = false }
    }

    @Test fun aQueuedRequestAndAnOldReplyCannotReachTheNextSession() {
        val app = RuntimeEnvironment.getApplication()
        val old = checkNotNull(ModuleSession.open(app))
        val first = Module.made
        Extensions.call(1, """{"wait":true}""", old)
        val next = checkNotNull(ModuleSession.open(app))
        try {
            old.end()
            Extensions.call(2, """{"wait":true}""", old)
            checkNotNull(first.pending).send(JSONObject())
            Extensions.call(3, """{"exact:answer":"true"}""", next)
            shadowOf(Looper.getMainLooper()).idle()
            assertSame(next, ModuleSession.current)
            assertNull(Module.made.pending)
            assertEquals(1, first.ends)
            assertEquals(listOf(OwnerReplies.Handed.Answer(3, "\"true\"")), Host.handed)
        } finally { next.end() }
    }

    // expo/exact#1596: an app's module reads the activity in front, as a package's ExtensionContext does, and null
    // while none is; it is read when asked, never the Context the session was made with.
    @Test fun theModuleSeesTheActivityInFrontAndNoneWhileNoneIs() {
        val first = Robolectric.buildActivity(Activity::class.java).setup()
        val next = Robolectric.buildActivity(Activity::class.java).setup()
        var front: Activity? = null
        var session: ModuleSession? = null
        try {
            Extensions.front = { front }
            session = checkNotNull(ModuleSession.open(first.get()))
            val context = Module.made.context
            assertNull(context.activity)
            front = first.get()
            assertSame(first.get(), context.activity)
            front = next.get()
            assertSame(next.get(), context.activity)
            front = null
            assertNull(context.activity)
        } finally {
            session?.end()
            Extensions.front = { Exact.reader?.get() }
            first.pause().stop().destroy()
            next.pause().stop().destroy()
        }
    }

    @Test fun theSessionOwnsCallsAndTimersAcrossReadersAndEndsOnce() {
        val reader = Robolectric.buildActivity(Activity::class.java).setup()
        val first = reader.get()
        val session = checkNotNull(ModuleSession.open(first))
        val module = Module.made
        try {
            assertSame(first.applicationContext, module.context.application)
            val caller = Thread { session.call("""{"op":"read"}""") }
            caller.start(); caller.join(5000)
            assertSame(caller, module.thread)
            assertEquals("!call failed", session.call("""{"throw":true}"""))
            assertEquals("!NotSupportedError: the app's module answers no native.call", session.call("""{"default":true}"""))
            session.app("""{"mode":"quiet"}""", true)
            assertEquals("quiet", module.appWords?.data?.get("mode"))
            assertEquals(true, module.appWords?.isNew)
            Extensions.call(1, """{"wait":true}""")
            val pending = checkNotNull(module.pending)
            module.context.after(1000.0) {
                module.context.changed("tick")
                pending.send(JSONObject().put("tick", 1))
                pending.fail("second reply")
            }
            shadowOf(Looper.getMainLooper()).idle()
            val timer = Host.handed.single() as OwnerReplies.Handed.After
            assertEquals(1123.0, timer.due, 0.0)
            reader.recreate()
            assertNotSame(first, reader.get())
            reader.pause().stop()
            assertSame(session, ModuleSession.current)
            Extensions.call(2, """{"echo":"🎵"}""")
            Extensions.call(3, """{"throw":true}""")
            Extensions.call(4, """{"extension":"absent","op":"read"}""")
            Extensions.call(5, """{"default":true}""")
            session.timer(timer.timer)
            reader.restart().start().resume()
            assertEquals(0, module.ends)
            shadowOf(Looper.getMainLooper()).idle()
            val delivered = listOf(
                timer, OwnerReplies.Handed.Answer(2, """{"echo":"🎵"}"""),
                OwnerReplies.Handed.Refused(3, "Error", "later failed"),
                OwnerReplies.Handed.Refused(4, "NotSupportedError", null),
                OwnerReplies.Handed.Refused(5, "NotSupportedError", "the app's module answers no native.later"),
                OwnerReplies.Handed.Changed("tick"), OwnerReplies.Handed.Answer(1, """{"tick":1}"""),
            )
            assertEquals(delivered.toSet(), Host.handed.toSet())
            session.end()
            session.end()
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(1, module.ends)
            assertNull(ModuleSession.current)
            session.timer(timer.timer)
            module.context.changed("after destruction")
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(delivered.size, Host.handed.size)
        } finally {
            session.end()
            reader.pause().stop().destroy()
        }
    }
}
