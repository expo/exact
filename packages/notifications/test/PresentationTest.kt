package exact.notifications

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import dev.exact.canvas.Activations
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Extensions
import dev.exact.canvas.Native
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.util.ReflectionHelpers

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], manifest = Config.NONE, shadows = [PresentationTest.Session::class])
class PresentationTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Session {
        companion object {
            val events = mutableListOf<JSONObject>()
            val lines = mutableListOf<String>()
            @JvmStatic @Implementation fun journal(line: String) { lines.add(line) }
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun changed(topic: String) {
                events += JSONObject(topic.removePrefix("exact-event:"))
            }
        }
    }

    @Test fun aNamedGroupKeepsItsChildrenAndSilencesItsSummary() {
        val app: Context = RuntimeEnvironment.getApplication()
        val manager = app.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel("trip", "Field trip", NotificationManager.IMPORTANCE_HIGH))
        fun record(tag: String) = JSONObject().put("identifier", tag).put("tag", tag).put("channel", "trip").put("title", tag)
        fun check(tag: String, group: String, summary: Boolean, alerts: Int) {
            val notification = manager.activeNotifications.single { it.tag == tag }.notification
            assertEquals(group, notification.group)
            assertEquals(summary, notification.flags and Notification.FLAG_GROUP_SUMMARY != 0)
            assertEquals(alerts, notification.groupAlertBehavior)
        }
        val fern = record("fern").put("group", "field-trip")
        val summary = record("summary").put("group", "field-trip").put("groupSummary", true)
        NotificationsModule.post(app, fern)
        check("fern", "field-trip", false, Notification.GROUP_ALERT_ALL)
        NotificationsModule.post(app, summary)
        check("summary", "field-trip", true, Notification.GROUP_ALERT_CHILDREN)
        NotificationsModule.post(app, fern.put("silent", true))
        check("fern", "field-trip", false, Notification.GROUP_ALERT_SUMMARY)
        Later.schedule(app, summary.put("silent", true).put("title", "Updated"), System.currentTimeMillis() + 5000)
        Later.rearm(app)
        Later.due(app, "summary")
        check("summary", "field-trip", true, Notification.GROUP_ALERT_CHILDREN)
        val updated = manager.activeNotifications.single { it.tag == "summary" }.notification
        assertEquals("Updated", updated.extras.getCharSequence(Notification.EXTRA_TITLE))
        try {
            NotificationsModule.builderHatch = { it.setGroup("hatch").setGroupSummary(true) }
            NotificationsModule.post(app, record("hatch-only"))
            check("hatch-only", "hatch", true, Notification.GROUP_ALERT_CHILDREN)
            NotificationsModule.post(app, record("inherited").put("silent", true))
            check("inherited", "hatch", true, Notification.GROUP_ALERT_CHILDREN)
            NotificationsModule.post(app, record("inherited").put("group", "authored"))
            check("inherited", "authored", true, Notification.GROUP_ALERT_CHILDREN)
            for (policy in listOf(Notification.GROUP_ALERT_SUMMARY, Notification.GROUP_ALERT_ALL)) {
                NotificationsModule.builderHatch = { it.setGroup("hatch").setGroupSummary(true).setGroupAlertBehavior(policy) }
                NotificationsModule.post(app, record("explicit"))
                check("explicit", "hatch", true, policy)
                NotificationsModule.post(app, record("explicit").put("silent", true))
                check("explicit", "hatch", true, Notification.GROUP_ALERT_CHILDREN)
            }
            NotificationsModule.builderHatch = { it.setGroup("").setGroupSummary(true) }
            NotificationsModule.post(app, record("empty"))
            check("empty", "", true, Notification.GROUP_ALERT_CHILDREN)
            NotificationsModule.builderHatch = { it.setGroup("") }
            NotificationsModule.post(app, record("empty").put("silent", true))
            check("empty", "silent", false, Notification.GROUP_ALERT_SUMMARY)
        } finally {
            NotificationsModule.builderHatch = null
        }
        NotificationsModule.post(app, record("solo").put("silent", true))
        check("solo", "silent", false, Notification.GROUP_ALERT_SUMMARY)
    }

    @Test fun anInvalidGroupIsRefusedBeforeThePermissionRequest() {
        val module = NotificationsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "notifications"))
        for ((words, name) in listOf(
            JSONObject().put("group", 7) to "android.group",
            JSONObject().put("group", "") to "android.group",
            JSONObject().put("groupSummary", false) to "android.groupSummary",
            JSONObject().put("group", "trip").put("groupSummary", "true") to "android.groupSummary"
        )) {
            var answer: JSONObject? = null
            module.later("showNotification", JSONObject().put("android", words)) { answer = it }
            assertEquals("TypeError", answer?.optString("error"))
            assertTrue(answer?.optString("message").orEmpty().startsWith(name))
        }
    }

    @Test fun aTimedPostReportsTheSystemHandoffOnlyWhileVisible() {
        val app: Context = RuntimeEnvironment.getApplication()
        val manager = app.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel("lab", "Reminders", NotificationManager.IMPORTANCE_DEFAULT))
        val post = JSONObject().put("identifier", "id").put("tag", "").put("channel", "lab").put("title", "Reminder")
        fun due() {
            Later.schedule(app, post, System.currentTimeMillis() + 5000)
            Later.due(app, "id")
        }
        val context = ExtensionContext(app, "notifications")
        // Other tests in this process may already have drawn their first frame.
        val wasReady = ReflectionHelpers.getStaticField<Boolean>(Activations::class.java, "ready")
        ReflectionHelpers.setStaticField(Activations::class.java, "ready", false)
        Extensions.started()
        try {
            for (i in 0 until 16) {
                ExtensionContext.emitWhileVisible("notifications", "notificationshow", JSONArray().put("tag-$i"), hold = 16)
            }
            due()
            assertEquals("id", manager.activeNotifications.single().tag)
            assertTrue(Session.events.isEmpty())
            Extensions.stopped(false)
            NotificationsModule(context)
            Activations.started()
            Extensions.started()
            assertEquals(listOf("notifications.notificationshow: dropped (tag-0)"), Session.lines)
            assertEquals((1..15).map { "tag-$it" } + "", Session.events.map { it.getJSONArray("args").getString(0) })
            val show = Session.events.last()
            Session.events.clear()
            assertEquals("notificationshow", show.getString("event"))
            val args = show.getJSONArray("args")
            assertEquals("", args.getString(0))
            val fields = args.getJSONObject(1)
            assertEquals(setOf("title", "body", "data", "tag"), fields.keys().asSequence().toSet())
            assertEquals("Reminder", fields.getString("title"))
            post.put("tag", "second").put("body", "After first frame").put("data", "lab-data")
            due()
            val next = Session.events.single().getJSONArray("args").getJSONObject(1)
            assertEquals("second", next.getString("tag"))
            assertEquals("After first frame", next.getString("body"))
            assertEquals("lab-data", next.getString("data"))
            manager.cancelAll()
            Extensions.stopped(false)
            due()
            assertEquals("id", manager.activeNotifications.single().tag)
            Extensions.started()
            manager.deleteNotificationChannel("lab")
            assertNull(manager.getNotificationChannel("lab"))
            due()
            assertEquals("a removed channel cannot present", 1, Session.events.size)
            manager.createNotificationChannel(NotificationChannel("lab", "Reminders", NotificationManager.IMPORTANCE_DEFAULT))
            shadowOf(manager).setNotificationsEnabled(false)
            due()
            assertEquals(1, Session.events.size)
        } finally {
            ReflectionHelpers.setStaticField(Activations::class.java, "ready", wasReady)
            Extensions.stopped(false)
            Session.events.clear()
            Session.lines.clear()
        }
    }
}
