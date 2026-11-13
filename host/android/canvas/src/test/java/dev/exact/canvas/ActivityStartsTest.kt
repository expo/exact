package dev.exact.canvas

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.provider.MediaStore
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** [ExtensionContext.startActivityForResult] through the process's [Extensions], over a Robolectric activity standing
 * for the reader's. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE)
class ActivityStartsTest {
    /** An activity whose starts the platform refuses with [refusal]. */
    class Refusing : Activity() {
        var refusal: RuntimeException? = null
        @Deprecated("Activity's own") override fun startActivityForResult(intent: Intent, requestCode: Int) {
            refusal?.let { throw it }
            @Suppress("DEPRECATION") super.startActivityForResult(intent, requestCode)
        }
    }

    private val camera = Intent(MediaStore.ACTION_IMAGE_CAPTURE)
    private val failures = ArrayList<String>()
    private val results = ArrayList<Pair<Int, Intent?>>()

    private fun <T : Activity> shown(type: Class<T>): T {
        val activity = Robolectric.buildActivity(type).setup().get()
        Extensions.front = { activity }
        Extensions.started()
        Extensions.resumed()
        return activity
    }

    private fun start() = Extensions.start(camera, { failures += it }) { code, data -> results += code to data }

    @After fun leave() {
        Extensions.paused()
        Extensions.stopped(false)
        Extensions.front = { Exact.reader?.get() }
    }

    /** The sequence of expo/exact#382: a package asks for CAMERA and starts the camera app from the answer, which
     * arrives while the dialog's close has the activity paused. The start waits for the resume, then is made. */
    @Test fun aStartFromAPermissionAnswerIsMadeWhenTheActivityResumes() {
        val activity = shown(Activity::class.java)
        Extensions.ask(arrayOf(android.Manifest.permission.CAMERA)) { answered ->
            assertEquals(setOf(android.Manifest.permission.CAMERA), answered)
            start()
        }
        val asked = shadowOf(activity).lastRequestedPermission
        assertArrayEquals(arrayOf(android.Manifest.permission.CAMERA), asked.requestedPermissions)
        // Robolectric records the dialog as an activity started for its result.
        assertEquals("android.content.pm.action.REQUEST_PERMISSIONS", shadowOf(activity).nextStartedActivityForResult.intent.action)
        // The dialog shows: the activity is paused and stays started (the dialog is translucent).
        Extensions.paused()
        Extensions.permissionsResult(asked.requestCode, asked.requestedPermissions, intArrayOf(PackageManager.PERMISSION_GRANTED))
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
        assertTrue(failures.isEmpty())

        Extensions.resumed()
        val made = shadowOf(activity).nextStartedActivityForResult
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, made.intent.action)
        assertTrue(failures.isEmpty())

        val photo = Intent()
        Extensions.activityResult(made.requestCode, Activity.RESULT_OK, photo)
        assertEquals(listOf(Activity.RESULT_OK to photo), results)
        // The result settles the call once.
        Extensions.activityResult(made.requestCode, Activity.RESULT_CANCELED, null)
        assertEquals(1, results.size)
    }

    /** The person leaves the app before the activity is in front again: the start ends as `AbortError`, and coming
     * back starts nothing. */
    @Test fun aWaitingStartEndsWhenTheAppStopsFirst() {
        val activity = shown(Activity::class.java)
        Extensions.paused()
        start()
        assertTrue(failures.isEmpty())
        Extensions.stopped(false)
        assertEquals(listOf("AbortError"), failures)

        Extensions.started()
        Extensions.resumed()
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
        assertTrue(results.isEmpty())
    }

    /** A call let go while its start waits withdraws it: the resume starts nothing and neither answer comes. Once
     * made, a start is past withdrawing. */
    @Test fun aWaitingStartWithdrawnIsNeverMade() {
        val activity = shown(Activity::class.java)
        Extensions.paused()
        val withdraw = start()
        assertTrue(withdraw())
        assertFalse(withdraw())
        Extensions.resumed()
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())
        assertTrue(failures.isEmpty())
        assertTrue(results.isEmpty())

        assertFalse(start()())
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, shadowOf(activity).nextStartedActivityForResult.intent.action)
    }

    /** A configuration change between the pause and the resume keeps the start for the activity made again. */
    @Test fun aWaitingStartOutlivesAConfigurationChange() {
        shown(Activity::class.java)
        Extensions.paused()
        start()
        Extensions.stopped(true)
        val again = Robolectric.buildActivity(Activity::class.java).setup().get()
        Extensions.front = { again }
        Extensions.started()
        Extensions.resumed()
        assertEquals(MediaStore.ACTION_IMAGE_CAPTURE, shadowOf(again).nextStartedActivityForResult.intent.action)
        assertTrue(failures.isEmpty())
    }

    /** With no activity showing nothing starts and nothing waits. */
    @Test fun aStartWithNoActivityShowingIsRefusedAtOnce() {
        val activity = shown(Activity::class.java)
        Extensions.paused()
        Extensions.stopped(false)
        start()
        assertEquals(listOf("InvalidStateError"), failures)
        Extensions.started()
        Extensions.resumed()
        assertNull(shadowOf(activity).peekNextStartedActivityForResult())

        Extensions.front = { null }
        start()
        assertEquals(listOf("InvalidStateError", "InvalidStateError"), failures)
    }

    /** A start the platform refuses is answered by its refusal and leaves no call waiting on a result: a
     * `SecurityException` (`ACTION_IMAGE_CAPTURE` from an app that declares CAMERA and does not hold it) is
     * `NotAllowedError`, no app for the intent `NotFoundError`, any other `UnknownError`. */
    @Test fun aRefusedStartIsAnsweredByItsRefusalAndForgotten() {
        val activity = shown(Refusing::class.java)
        for ((refusal, name) in listOf(
            SecurityException("Permission Denial: android.permission.CAMERA") to "NotAllowedError",
            android.content.ActivityNotFoundException() to "NotFoundError",
            IllegalStateException() to "UnknownError",
        )) {
            activity.refusal = refusal
            start()
            assertEquals(name, failures.last())
        }
        assertEquals(3, failures.size)
        for (code in 0x5000 until 0x6000) Extensions.activityResult(code, Activity.RESULT_OK, null)
        assertTrue(results.isEmpty())
    }
}
