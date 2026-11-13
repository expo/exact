package exact.calendar

import android.Manifest
import android.content.ContentProvider
import android.content.ContentValues
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Looper
import android.provider.CalendarContract
import android.provider.CalendarContract.Instances
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.ExtensionInvokeTest.Host
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import java.util.TimeZone
import java.util.concurrent.ExecutorService
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowContentResolver

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [Host::class])
class EventStoreTest {
    private val originalZone = TimeZone.getDefault()
    private val provider = Provider()
    private lateinit var store: EventStore
    private lateinit var worker: ExecutorService

    private class Provider : ContentProvider() {
        val rows = ArrayList<Map<String, Any?>>()
        /** `Events` rows by id, which the writes read first. */
        val events = HashMap<Long, Map<String, Any?>>()
        val writes = ArrayList<Uri>()
        override fun onCreate() = true
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?): Cursor {
            val columns = requireNotNull(projection)
            if (uri.pathSegments.first() == "events") return MatrixCursor(columns).apply {
                events[uri.lastPathSegment!!.toLong()]?.let { row -> addRow(columns.map { row[it] }.toTypedArray()) }
            }
            val (from, to) = uri.pathSegments.takeLast(2).map(String::toLong)
            return MatrixCursor(columns).apply {
                for (row in rows) {
                    // CalendarProvider2's SQL_WHERE_INSTANCES_BETWEEN compares the stored UTC instants.
                    if ((row[Instances.BEGIN] as Long) <= to && (row[Instances.END] as Long) >= from) {
                        addRow(columns.map { row[it] }.toTypedArray())
                    }
                }
            }
        }
        override fun getType(uri: Uri): String? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri = error("read only")
        override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?): Int = 1.also { writes += uri }
        override fun delete(uri: Uri, selection: String?, args: Array<out String>?): Int = 1.also { writes += uri }
    }

    @Before fun setUp() {
        val app = RuntimeEnvironment.getApplication()
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions = arrayOf(Manifest.permission.READ_CALENDAR)
        shadowOf(app).grantPermissions(Manifest.permission.READ_CALENDAR)
        provider.attachInfo(app, ProviderInfo().apply { authority = CalendarContract.AUTHORITY })
        ShadowContentResolver.registerProviderInternal(CalendarContract.AUTHORITY, provider)
        store = EventStore(ExtensionContext(app, "calendar"), {}, {})
        worker = EventStore::class.java.getDeclaredField("work").apply { isAccessible = true }.get(store) as ExecutorService
    }

    @After fun tearDown() {
        store.stopObserving()
        worker.shutdownNow()
        assertTrue(worker.awaitTermination(10, TimeUnit.SECONDS))
        TimeZone.setDefault(originalZone)
    }

    private fun inZone(zone: String) { TimeZone.setDefault(TimeZone.getTimeZone(zone)) }
    private fun instant(text: String) = Instant.parse(text).toEpochMilli()
    private fun local(text: String) = LocalDateTime.parse(text).atZone(ZoneId.systemDefault()).toInstant().toEpochMilli()

    private fun event(id: Long, start: String, end: String, allDay: Boolean = true,
        rule: String? = null, series: Long? = null, original: String? = null, zone: String = "UTC",
        originalAllDay: Boolean = allDay, dates: String? = null) {
        provider.rows += mapOf(
            Instances.EVENT_ID to id, Instances.BEGIN to instant(start), Instances.END to instant(end),
            Instances.ALL_DAY to if (allDay) 1 else 0, Instances.TITLE to "Event $id", Instances.EVENT_TIMEZONE to zone,
            Instances.CALENDAR_ID to 1L, Instances.STATUS to CalendarContract.Events.STATUS_CONFIRMED,
            Instances.AVAILABILITY to CalendarContract.Events.AVAILABILITY_BUSY,
            Instances.RRULE to rule, Instances.ORIGINAL_ID to series, Instances.ORIGINAL_INSTANCE_TIME to original?.let(::instant),
            Instances.ORIGINAL_ALL_DAY to if (original == null) null else if (originalAllDay) 1 else 0, Instances.RDATE to dates,
        )
    }

    private fun page(start: String, end: String, limit: Int = 500, after: String? = null): JSONObject {
        val args = JSONObject().put("start", local(start)).put("end", local(end)).put("limit", limit)
        after?.let { args.put("after", it) }
        var answer: JSONObject? = null
        store.later("calendarEvents", args) { answer = it }
        // Drain the store's query thread before delivering its main-thread reply.
        worker.submit {}.get(10, TimeUnit.SECONDS)
        shadowOf(Looper.getMainLooper()).idle()
        val result = requireNotNull(answer) { "calendarEvents did not reply" }
        assertFalse(result.toString(), result.has("error"))
        return result
    }

    private fun allDayInRange(zone: String, start: String, end: String) {
        inZone(zone)
        event(1, "2026-10-23T00:00:00Z", "2026-10-24T00:00:00Z")
        val events = page(start, end).getJSONArray("events")
        assertEquals("$zone: $start to $end", 1, events.length())
        val event = events.getJSONObject(0)
        assertEquals("2026-10-23T00:00:00", event.getString("start"))
        assertEquals(local("2026-10-23T00:00:00"), event.getLong("utcStart"))
        assertEquals(local("2026-10-24T00:00:00"), event.getLong("utcEnd"))
        assertTrue(event.isNull("recurrenceId"))
    }

    @Test fun westernEveningIncludesTodaysAllDayEvent() =
        allDayInRange("America/Los_Angeles", "2026-10-23T18:00:00", "2026-10-24T00:00:00")

    @Test fun nextWeekFromWesternEveningIncludesTodaysAllDayEvent() =
        allDayInRange("America/Los_Angeles", "2026-10-23T18:00:00", "2026-10-30T18:00:00")

    @Test fun easternMorningIncludesTodaysAllDayEvent() =
        allDayInRange("Asia/Tokyo", "2026-10-23T01:00:00", "2026-10-23T03:00:00")

    private fun recurringAllDay(zone: String) {
        inZone(zone)
        event(1, "2026-10-23T00:00:00Z", "2026-10-24T00:00:00Z", rule = "FREQ=YEARLY")
        val event = page("2026-10-23T00:00:00", "2026-10-24T00:00:00").getJSONArray("events").getJSONObject(0)
        assertEquals("2026-10-23T00:00:00", event.getString("recurrenceId"))
        assertEquals(event.getString("start"), event.getString("recurrenceId"))
    }

    @Test fun westernAllDayRecurrenceNamesTheLocalDate() = recurringAllDay("America/Los_Angeles")
    @Test fun easternAllDayRecurrenceNamesTheLocalDate() = recurringAllDay("Asia/Tokyo")

    @Test fun movedAllDayRecurrenceKeepsItsOriginalDate() {
        inZone("America/Los_Angeles")
        event(2, "2026-10-24T00:00:00Z", "2026-10-25T00:00:00Z", series = 1, original = "2026-10-23T00:00:00Z")
        val event = page("2026-10-24T00:00:00", "2026-10-25T00:00:00").getJSONArray("events").getJSONObject(0)
        assertEquals("1", event.getString("id"))
        assertEquals("2026-10-24T00:00:00", event.getString("start"))
        assertEquals("2026-10-23T00:00:00", event.getString("recurrenceId"))
    }

    @Test fun timedOccurrenceChangedToAllDayKeepsItsOriginalTime() {
        inZone("UTC")
        event(2, "2026-10-24T00:00:00Z", "2026-10-25T00:00:00Z", series = 1,
            original = "2026-10-23T09:00:00Z", originalAllDay = false)
        val event = page("2026-10-24T00:00:00", "2026-10-25T00:00:00").getJSONArray("events").getJSONObject(0)
        assertEquals("1", event.getString("id"))
        assertEquals("2026-10-24T00:00:00", event.getString("start"))
        assertEquals("2026-10-23T09:00:00", event.getString("recurrenceId"))
    }

    @Test fun allDayOccurrenceChangedToTimedKeepsItsOriginalDate() {
        inZone("Asia/Tokyo")
        event(2, "2026-10-23T17:00:00Z", "2026-10-23T18:00:00Z", allDay = false,
            series = 1, original = "2026-10-22T00:00:00Z", originalAllDay = true, zone = "America/Los_Angeles")
        val event = page("2026-10-24T02:00:00", "2026-10-24T03:00:00").getJSONArray("events").getJSONObject(0)
        assertEquals("1", event.getString("id"))
        assertEquals("2026-10-23T10:00:00", event.getString("start"))
        assertEquals("2026-10-22T00:00:00", event.getString("recurrenceId"))
    }

    @Test fun dateListRecurrencesHaveOriginalStarts() {
        inZone("America/Los_Angeles")
        event(1, "2026-10-23T00:00:00Z", "2026-10-24T00:00:00Z", dates = "20261023T000000Z,20261024T000000Z")
        event(2, "2026-10-23T16:00:00Z", "2026-10-23T17:00:00Z", allDay = false,
            dates = "20261023T160000Z,20261024T160000Z", zone = "America/Los_Angeles")
        val events = page("2026-10-23T00:00:00", "2026-10-24T00:00:00").getJSONArray("events")
        assertEquals(2, events.length())
        assertEquals("2026-10-23T00:00:00", events.getJSONObject(0).getString("recurrenceId"))
        assertEquals("2026-10-23T09:00:00", events.getJSONObject(1).getString("recurrenceId"))
    }

    @Test fun widenedQueryKeepsHalfOpenBoundsAndPagination() {
        inZone("Asia/Tokyo")
        event(1, "2026-10-22T00:00:00Z", "2026-10-23T00:00:00Z")
        event(2, "2026-10-23T00:00:00Z", "2026-10-24T00:00:00Z", rule = "FREQ=YEARLY")
        event(3, "2026-10-23T00:00:00Z", "2026-10-24T00:00:00Z", rule = "FREQ=YEARLY")
        event(4, "2026-10-24T00:00:00Z", "2026-10-25T00:00:00Z")
        val first = page("2026-10-23T00:00:00", "2026-10-24T00:00:00", limit = 1)
        assertEquals(1, first.getJSONArray("events").length())
        assertEquals("2", first.getJSONArray("events").getJSONObject(0).getString("id"))
        val second = page("2026-10-23T00:00:00", "2026-10-24T00:00:00", limit = 1, after = first.getString("next"))
        assertEquals(1, second.getJSONArray("events").length())
        assertEquals("3", second.getJSONArray("events").getJSONObject(0).getString("id"))
        assertTrue(second.isNull("next"))
    }

    @Test fun timedRecurrencesKeepTheirInstantsAndNamedZone() {
        inZone("Asia/Tokyo")
        event(1, "2026-10-23T16:00:00Z", "2026-10-23T17:00:00Z", allDay = false,
            rule = "FREQ=DAILY", zone = "America/Los_Angeles")
        event(2, "2026-10-23T17:00:00Z", "2026-10-23T18:00:00Z", allDay = false,
            series = 3, original = "2026-10-22T16:00:00Z", zone = "America/Los_Angeles")
        event(4, "2026-10-23T18:00:00Z", "2026-10-23T19:00:00Z", allDay = false)
        val events = page("2026-10-24T01:00:00", "2026-10-24T03:00:00").getJSONArray("events")
        assertEquals(2, events.length())
        assertEquals("2026-10-23T09:00:00", events.getJSONObject(0).getString("recurrenceId"))
        val moved = events.getJSONObject(1)
        assertEquals("2026-10-23T10:00:00", moved.getString("start"))
        assertEquals("2026-10-22T09:00:00", moved.getString("recurrenceId"))
        assertEquals(instant("2026-10-23T17:00:00Z"), moved.getLong("utcStart"))
    }

    private fun write(name: String, args: JSONObject): JSONObject {
        val app = RuntimeEnvironment.getApplication()
        shadowOf(app.packageManager).getInternalMutablePackageInfo(app.packageName).requestedPermissions =
            arrayOf(Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CALENDAR)
        shadowOf(app).grantPermissions(Manifest.permission.WRITE_CALENDAR)
        var answer: JSONObject? = null
        store.later(name, args) { answer = it }
        worker.submit {}.get(10, TimeUnit.SECONDS)
        shadowOf(Looper.getMainLooper()).idle()
        return requireNotNull(answer) { "$name did not reply" }
    }

    /** A series may be a list of dates alone (RFC 5545 §3.8.5.2), with no rule; writing it is refused as a ruled one is. */
    private fun seriesIsNotWritten(rule: String?, dates: String?) {
        provider.events[7L] = mapOf(CalendarContract.Events.CALENDAR_ID to 1L, CalendarContract.Events.RRULE to rule,
            CalendarContract.Events.RDATE to dates, CalendarContract.Events.ORIGINAL_ID to null)
        assertEquals("NotSupportedError", write("removeEvent", JSONObject().put("id", "7")).optString("error"))
        val event = JSONObject().put("id", "7").put("title", "Moved").put("start", "2026-11-10T09:00:00").put("duration", "PT1H")
        assertEquals("NotSupportedError", write("saveEvent", JSONObject().put("event", event)).optString("error"))
        assertEquals(emptyList<Uri>(), provider.writes)
    }

    @Test fun aRuledSeriesIsNotWritten() = seriesIsNotWritten("FREQ=WEEKLY", null)

    @Test fun aDateListSeriesIsNotWritten() = seriesIsNotWritten(null, "20261110T090000Z,20261117T090000Z")
}
