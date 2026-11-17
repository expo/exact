package exact.payments

import android.app.Activity
import android.os.Looper
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.time.Duration
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

/** D6: an authorization whose handler passes D5's budget ends the request for the app. The send answers `fail` with
 * `authorize-timeout` then, though Google's sheet stays up, and an authorization after it calls no handler. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [AuthorizeTimeoutTest.Unanswered::class])
class AuthorizeTimeoutTest {
    /** A session that takes every handler call and never answers it. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Unanswered {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun invoke(body: String): Int { invoked++; return 1 }
            @JvmStatic @Implementation fun invoked(id: Int): String? = null
            var invoked = 0
        }
    }

    @Test fun anAuthorizationPastItsBudgetAnswersTheSendAsFailed() {
        Unanswered.invoked = 0
        val module = PaymentsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "payments"))
        val answers = ArrayList<JSONObject>()
        val args = JSONObject().put("onpaymentauthorized", "authorize").put("details", JSONObject().put("id", "order-1"))
        val sheetClass = Class.forName("exact.payments.PaymentsModule\$Sheet")
        val sheet = sheetClass.declaredConstructors.single().apply { isAccessible = true }
            .newInstance(7, "order-1", args, { a: JSONObject -> answers += a })
        ReflectionHelpers.setField(module, "sheet", sheet)
        val google = ArrayList<String>()

        module.authorized(JSONObject()) { google += it }
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(26))
        assertEquals(1, Unanswered.invoked)
        assertTrue(google.single().contains("\"ERROR\""))
        assertEquals("the send is answered at the budget", 1, answers.size)
        assertEquals("fail", answers[0].getString("result"))
        assertEquals("authorize-timeout", answers[0].getString("message"))

        module.authorized(JSONObject()) { google += it }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("a later authorization calls no handler", 1, Unanswered.invoked)
        assertTrue(google.last().contains("This order was closed."))
        ReflectionHelpers.callInstanceMethod<Unit>(module, "finish",
            ReflectionHelpers.ClassParameter.from(sheetClass, sheet),
            ReflectionHelpers.ClassParameter.from(Int::class.javaPrimitiveType, Activity.RESULT_CANCELED),
            ReflectionHelpers.ClassParameter.from(com.google.android.gms.wallet.PaymentData::class.java, null),
            ReflectionHelpers.ClassParameter.from(android.content.Intent::class.java, null))
        assertEquals("closing the sheet answers nothing more", 1, answers.size)
    }
}
