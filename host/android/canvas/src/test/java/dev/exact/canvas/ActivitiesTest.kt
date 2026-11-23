package dev.exact.canvas

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Looper
import android.service.notification.StatusBarNotification
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

/** What a live activity posts (LLP 1126 D13) on Android 14 and 15, where the system bundles notifications outside an app group. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34, 35], manifest = Config.NONE, shadows = [ActivitiesTest.Host::class])
class ActivitiesTest {
    @Implements(Native::class, isInAndroidSdk = false)
    class Host {
        companion object {
            val reports = ArrayList<JSONObject>()
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun activities(report: String) { reports += JSONObject(report) }
        }
    }

    private val context = RuntimeEnvironment.getApplication()
    private val manager = context.getSystemService(NotificationManager::class.java)
    private var tickets = 0L

    @Before fun setUp() {
        Host.reports.clear()
        manager.createNotificationChannel(NotificationChannel("trips", "Trips", NotificationManager.IMPORTANCE_DEFAULT))
        Activities.application = context
        Activities.rows = mapOf("departure" to Activities.Row("departure", "trips", "label", "detail", "done", null, "at"))
    }

    private fun state(label: String, done: Double) = JSONObject().put("label", label).put("detail", "Platform 2").put("done", done).put("at", System.currentTimeMillis() + 600_000)

    /** Send [command], let the shade settle, and return its answer. */
    private fun send(command: String, vararg args: Any): JSONObject {
        val ticket = ++tickets
        Activities.request(JSONObject().put("ticket", ticket).put("command", command).put("args", JSONArray(args.toList())).toString())
        shadowOf(Looper.getMainLooper()).idle()
        return Host.reports.single { it.optLong("ticket") == ticket }
    }

    private fun start(label: String) = send("startActivity", "departure", state(label, 0.25)).getJSONObject("body").getString("id")

    private fun posts(id: String) = manager.activeNotifications.filter { it.tag == id }
    private fun StatusBarNotification.summary() = notification.flags and Notification.FLAG_GROUP_SUMMARY != 0
    private fun StatusBarNotification.ongoing() = notification.flags and Notification.FLAG_ONGOING_EVENT != 0
    private fun listed() = Host.reports.last { it.has("activities") }.getJSONArray("activities").length()

    @Test fun eachActivityIsAGroupOfItsOwnWhoseSummaryNeverAlertsNorAsksForPromotion() {
        val ids = listOf(start("Departure 1"), start("Departure 2"), start("Departure 3"))
        for (id in ids) {
            val (summary, child) = posts(id).partition { it.summary() }.let { it.first.single() to it.second.single() }
            assertEquals(id, child.notification.group)
            assertEquals(id, summary.notification.group)
            assertTrue(child.ongoing() && summary.ongoing())
            assertTrue(child.notification.extras.getBoolean(Notification.EXTRA_REQUEST_PROMOTED_ONGOING))
            assertFalse(summary.notification.extras.getBoolean(Notification.EXTRA_REQUEST_PROMOTED_ONGOING))
            assertEquals(Notification.GROUP_ALERT_CHILDREN, summary.notification.groupAlertBehavior)
            assertEquals(child.notification.channelId, summary.notification.channelId)
        }
        assertEquals(6, manager.activeNotifications.size)
        assertEquals(3, listed())
    }

    @Test fun anUpdateReplacesTheActivitysPostAndLeavesItsSummary() {
        val id = start("Departure 1")
        val summary = posts(id).single { it.summary() }.notification
        send("updateActivity", id, state("Departure delayed", 0.5), JSONObject.NULL, JSONObject.NULL)
        val after = posts(id)
        assertEquals(2, after.size)
        assertEquals("Departure delayed", after.single { !it.summary() }.notification.extras.getString(Notification.EXTRA_TITLE))
        assertSame(summary, after.single { it.summary() }.notification)
    }

    @Test fun aKeptEndTimesOutWithItsSummaryAndAnImmediateEndTakesBoth() {
        val kept = start("Departure 1")
        val gone = start("Departure 2")
        send("endActivity", kept, state("Arrived", 1.0), "default")
        val ended = posts(kept)
        assertEquals(2, ended.size)
        assertTrue(ended.none { it.ongoing() })
        assertEquals(1, ended.map { it.notification.timeoutAfter }.toSet().size)
        assertTrue(ended[0].notification.timeoutAfter > 0)
        send("endActivity", gone, state("Arrived", 1.0), "immediate")
        assertTrue(posts(gone).isEmpty())
        assertEquals(1, listed())
    }

    /** Android sheds an app's updates past five a second, but not one that completes a progress: a kept end's summary
     * update may be shed while the activity's is not. The next list ends the summary as the activity is. */
    @Test fun aKeptEndsSummaryThatStayedOngoingEndsAtTheNextList() {
        val id = start("Departure 1")
        send("endActivity", id, state("Arrived", 1.0), "default")
        val summary = posts(id).single { it.summary() }
        manager.notify(id, summary.id, Notification.Builder(context, "trips").setSmallIcon(android.R.drawable.ic_dialog_info)
            .setGroup(id).setGroupSummary(true).setOngoing(true).build())
        Activities.resumed()
        val after = posts(id).single { it.summary() }.notification
        assertEquals(0, after.flags and Notification.FLAG_ONGOING_EVENT)
        assertTrue(after.timeoutAfter > 0)
    }

    /** A vendor's shade may leave the summary when the person dismisses the activity: the next list cancels it. */
    @Test fun aSummaryWhoseActivityIsGoneIsCancelled() {
        val id = start("Departure 1")
        val child = posts(id).single { !it.summary() }
        manager.cancel(id, child.id)
        Activities.resumed()
        assertTrue(posts(id).isEmpty())
        assertEquals(0, listed())
    }
}
