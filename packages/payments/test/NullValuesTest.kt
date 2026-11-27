package exact.payments

import android.os.Looper
import dev.exact.canvas.ExtensionContext
import dev.exact.canvas.Native
import java.time.Duration
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
import org.robolectric.util.ReflectionHelpers

/** Explicit JSON nulls and primitive `data` values in payment handler replies must preserve existing totals/items,
 * avoid bogus `"null"` errors, and JSON-encode primitive `data` values cleanly. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [31], manifest = Config.NONE, shadows = [NullValuesTest.Handler::class])
class NullValuesTest {
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

    @Before fun reset() {
        Handler.invoked = 0
        Handler.answer = null
    }

    private fun setupSheet(module: PaymentsModule, args: JSONObject): Any {
        val sheetClass = Class.forName("exact.payments.PaymentsModule\$Sheet")
        val sheet = sheetClass.declaredConstructors.single().apply { isAccessible = true }
            .newInstance(9, "order-nulls", args, { _: JSONObject -> })
        ReflectionHelpers.setField(module, "sheet", sheet)
        return sheet
    }

    @Test fun shippingUpdateWithExplicitNullsPreservesTotalsAndEmitsNoError() {
        val module = PaymentsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "payments"))
        val details = JSONObject()
            .put("id", "order-nulls")
            .put("total", JSONObject().put("label", "Shop").put("amount", JSONObject().put("currency", "EUR").put("value", "25.00")))
            .put("displayItems", JSONArray().put(JSONObject().put("label", "Book").put("amount", JSONObject().put("currency", "EUR").put("value", "25.00"))))
        val args = JSONObject().put("onshippingaddresschange", "onShip").put("details", details)
        setupSheet(module, args)

        Handler.answer = JSONObject().put("text", """{
            "total": null,
            "displayItems": null,
            "shippingOptions": [{"id": "std", "label": "Standard", "amount": {"currency": "EUR", "value": "5.00"}, "selected": true}],
            "error": null,
            "shippingAddressErrors": {"postalCode": null}
        }""").toString()

        var response: String? = null
        module.changed(JSONObject().put("callbackTrigger", "SHIPPING_ADDRESS")) { response = it }
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))

        val out = JSONObject(requireNotNull(response))
        assertFalse("null error must not trigger SHIPPING_ADDRESS_UNSERVICEABLE: $out", out.has("error"))
        val tx = out.getJSONObject("newTransactionInfo")
        assertEquals("25.00", tx.getString("totalPrice"))
        assertEquals("EUR", tx.getString("currencyCode"))
        assertEquals(1, tx.getJSONArray("displayItems").length())
        assertTrue(out.has("newShippingOptionParameters"))
    }

    @Test fun authorizeDataNullOmittedAndPrimitivesProperlyJsonEncoded() {
        val module = PaymentsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "payments"))
        val args = JSONObject().put("onpaymentauthorized", "authorize").put("details", JSONObject().put("id", "order-nulls"))

        // data: null -> omitted
        val sheet1 = setupSheet(module, args)
        Handler.answer = JSONObject().put("text", """{"result":"success","data":null}""").toString()
        var g1: String? = null
        module.authorized(JSONObject()) { g1 = it }
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertEquals("SUCCESS", JSONObject(requireNotNull(g1)).getString("transactionState"))
        val r1 = ReflectionHelpers.getField<JSONObject>(sheet1, "result")
        assertFalse("data: null must be omitted, not stringified as \"null\"", r1.has("data"))

        // data: "order-123" -> JSON-quoted string "\"order-123\""
        val sheet2 = setupSheet(module, args)
        Handler.answer = JSONObject().put("text", """{"result":"success","data":"order-123"}""").toString()
        var g2: String? = null
        module.authorized(JSONObject()) { g2 = it }
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))
        assertEquals("SUCCESS", JSONObject(requireNotNull(g2)).getString("transactionState"))
        val r2 = ReflectionHelpers.getField<JSONObject>(sheet2, "result")
        assertEquals("\"order-123\"", r2.getString("data"))

        // Numbers, booleans, objects, arrays
        assertEquals("42", jsonText(42))
        assertEquals("true", jsonText(true))
        assertEquals("""{"ok":1}""", jsonText(JSONObject().put("ok", 1)))
        assertEquals("[1,2]", jsonText(JSONArray().put(1).put(2)))
        assertNull(jsonText(JSONObject.NULL))
    }

    @Test fun retryWithNullErrorFallsBackToDefaultMessage() {
        val module = PaymentsModule(ExtensionContext(RuntimeEnvironment.getApplication(), "payments"))
        val args = JSONObject().put("onpaymentauthorized", "authorize").put("details", JSONObject().put("id", "order-nulls"))
        setupSheet(module, args)

        Handler.answer = JSONObject().put("text", """{"retry":{"error":null,"shippingAddress":{"postalCode":null}}}""").toString()
        var google: String? = null
        module.authorized(JSONObject()) { google = it }
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(100))

        val out = JSONObject(requireNotNull(google))
        assertEquals("ERROR", out.getString("transactionState"))
        assertEquals("Try another card.", out.getJSONObject("error").getString("message"))
    }
}
