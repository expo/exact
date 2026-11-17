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

/** D6: a call let go while its final handler runs lets that handler finish; the sheet stays, and an authorization
 * after it calls no handler. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [CancelTest.Handler::class])
class CancelTest {
    /** A session whose handler answers when the test says. */
    @Implements(Native::class, isInAndroidSdk = false)
    class Handler {
        companion object {
            @JvmStatic @Implementation fun __staticInitializer__() {}
            @JvmStatic @Implementation fun invoke(body: String): Int { invoked++; return invoked }
            @JvmStatic @Implementation fun invoked(id: Int): String? = answer
            var invoked = 0
            var answer: String? = null
        }
    }

    @Test fun aCallLetGoWhileAuthorizingRefusesTheNextAuthorization() {
        val module = PaymentsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "payments"))
        val answers = ArrayList<JSONObject>()
        val args = JSONObject().put("onpaymentauthorized", "authorize").put("details", JSONObject().put("id", "order-1"))
        val sheetClass = Class.forName("exact.payments.PaymentsModule\$Sheet")
        val sheet = sheetClass.declaredConstructors.single().apply { isAccessible = true }
            .newInstance(7, "order-1", args, { a: JSONObject -> answers += a })
        ReflectionHelpers.setField(module, "sheet", sheet)
        val google = ArrayList<String>()

        module.authorized(JSONObject()) { google += it }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, Handler.invoked)
        module.cancel(7)
        Handler.answer = JSONObject().put("text", JSONObject().put("result", "fail").toString()).toString()
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertTrue("the running handler's answer reaches Google", google.single().contains("\"ERROR\""))

        module.authorized(JSONObject()) { google += it }
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("a later authorization calls no handler", 1, Handler.invoked)
        assertTrue(google.last().contains("This order was closed."))
        ReflectionHelpers.callInstanceMethod<Unit>(module, "finish",
            ReflectionHelpers.ClassParameter.from(sheetClass, sheet),
            ReflectionHelpers.ClassParameter.from(Int::class.javaPrimitiveType, Activity.RESULT_CANCELED),
            ReflectionHelpers.ClassParameter.from(com.google.android.gms.wallet.PaymentData::class.java, null),
            ReflectionHelpers.ClassParameter.from(android.content.Intent::class.java, null))
        assertEquals("the call let go is answered nothing", 0, answers.size)
    }
}
