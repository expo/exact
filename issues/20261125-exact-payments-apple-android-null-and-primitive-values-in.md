# @exact/payments (Apple & Android): null and primitive values in payment handler replies crash Apple Pay after authorization, silently reset Google Pay totals to $0.00 USD, and reject valid shipping updates

**Status:** Open
**Systems:** packages/payments (Apple PassKit, Android Google Pay)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Numeric narrowing, Unicode slicing and escaping)

Verification: verified by running.

At `9c0431fd5`, `PaymentsModule` on Apple (`packages/payments/apple/Payments.swift:285, 290-300, 362`) and Android (`packages/payments/android/Payments.kt:114-115, 246-247, 252-253, 279-281, 289`) mishandles explicit JSON `null` (`NSNull` / `JSONObject.NULL`) and JSON primitive `data` values returned by an app's `tools` payment handlers.

### Reachable trigger
An app's `tools` handler (e.g., `onpaymentauthorized` or `onshippingaddresschange` / `onshippingoptionchange` / `onpaymentmethodchange`) returns a plain JavaScript object from `app.ts`:
- `js/src/prelude.js:796-812` serializes the return value verbatim with `JSON.stringify`, and `runner/src/runner/modules/invoke.rs:461-470, 483` forwards it unchanged as `{"text": text}` (unlike outgoing call arguments, which `runner/src/runner/modules/bound.rs:608-617` strips of `null` fields).
- `Payments.swift:275-279` parses `answer["text"]` with `JSONSerialization.jsonObject` (producing `NSNull()` for JSON `null`, and `String`/`NSNumber` for primitives).
- `Payments.kt:216-218` parses `answer.optString("text")` with `org.json.JSONObject` (producing `JSONObject.NULL` for JSON `null`).

Triggering replies:
1. `onpaymentauthorized` returns `{ result: "success", data: null }` or `{ result: "success", data: "order-123" }` (or any JSON primitive: number/boolean).
2. `onshippingaddresschange` / `onshippingoptionchange` / `onpaymentmethodchange` returns a `PaymentDetailsUpdate` with explicit `null` for unchanged optional fields, e.g. `{ total: null, shippingOptions: [...], error: null }`, or `onpaymentauthorized` returns `{ retry: { error: null, ... } }`.

### Actual vs. expected result
1. **Apple post-authorization crash (`Payments.swift:362`):**
   `if let data = value?["data"], let text = try? JSONSerialization.data(withJSONObject: data)` passes `NSNull()` or any JSON primitive (`String`, `NSNumber`, `Bool`) directly to `JSONSerialization.data(withJSONObject:)` without `.fragmentsAllowed` or an `isValidJSONObject` / `NSNull` guard. Foundation raises an Objective-C `NSInvalidArgumentException` ("Invalid top-level type in JSON write"), which Swift's `try?` cannot catch, aborting the process (`SIGABRT`) right after biometric authorization (and potentially after the server charged the card) before `completion(...)` runs at line 365.
2. **Android `data` serialization corruption (`Payments.kt:289`):**
   `value?.opt("data")?.let { out.put("data", it.toString()) }` treats `JSONObject.NULL` as non-null and writes the 4-character string `"null"` (decoding as `some("null")` instead of `none`), and writes string primitives unquoted (`"order-123"` instead of `"\"order-123\""`), producing invalid JSON for `option<json<PaymentCompleteData>>`.
3. **Android silent `$0.00 USD` total reset and bogus shipping/retry errors (`Payments.kt:114-115, 246-247, 252-253, 279-281`):**
   `if (u.has(k)) details.put(k, u.get(k))` is `true` for `JSONObject.NULL`, overwriting `details["total"]` with `JSONObject.NULL` (and wiping `displayItems`/`shippingOptions` when `null`). `transaction()` (line 119) then reads `details.optJSONObject("total") == null` and falls back via lines 114-115 to `totalPrice: "0.00"` and `currencyCode: "USD"`. Additionally, Android's `org.json.JSONObject.optString(...)` coerces `JSONObject.NULL` to `"null"`, so `{ error: null }` at line 252 triggers `SHIPPING_ADDRESS_UNSERVICEABLE` with message `"null"` (and `retry.optString("error")` at line 281 and `shippingAddress`/`shippingAddressErrors` map reads at lines 252 and 279 likewise emit `"null"`).
4. **Apple sheet dismissal on explicit `null` (`Payments.swift:285, 290-300`):**
   `where update?[key] != nil` evaluates to `true` for `Optional.some(NSNull())`, overwriting `details["total"]` with `NSNull()`. `summary(details)` (lines 254-259) then throws `AmountError.invalid`, entering the `catch` block at lines 293-300 which dismisses the Apple Pay sheet and replies `TypeError` (and explicit `null` for `displayItems`/`shippingOptions` silently clears accepted items/methods to `[]`).

**Expected:** Per `packages/payments/exact.json:84` (`PaymentResult.data: option<json<PaymentCompleteData>>`), `runner/src/runner/modules/sheet.rs:533-536, 593-596`, `host/web-js/sheets.js:181, 198`, `packages/payments/web/index.js:32, 35-37, 138`, and `host/apple/modules/ExactNativeModule.swift:199-206`, explicit `null` fields in `PaymentDetailsUpdate` and `retry` must be ignored so previously accepted details remain intact and no `"null"` error is generated, and `PaymentResult.data` must omit/pass null for `null` and validly JSON-encode any JSON value (objects, arrays, and primitives) without throwing an Objective-C exception.

### Verification and sibling sweep
- **Verification:** Verified Apple's `NSNull` and primitive behavior by running `swift -e` against Foundation's `JSONSerialization`: `obj["total"] != nil` printed `true` with `NSNull`, `isValidJSONObject` returned `false` for both `NSNull` and `"order-123"` (and `true` when wrapped in `[value]`), and `try? JSONSerialization.data(withJSONObject: data)` terminated with `NSInvalidArgumentException` (`SIGABRT`, exit 134) for both `{"data": null}` and `{"data": "order-123"}`. Verified Android `Payments.kt` by reading and against Android `org.json.JSONObject` semantics (`has`, `opt`, and `optString` coercing `JSONObject.NULL` to `"null"`).
- **Siblings checked:** `packages/payments/web/index.js:32, 35-37, 138` (`without` filters `v != null`; `data == null ? null : JSON.stringify(data)`), `runner/src/runner/modules/sheet.rs:533-536, 593-596`, and `host/web-js/sheets.js:181, 198` all handle `null` and primitive `data` properly. `packages/language-model/apple/LanguageModel.swift:419-429` + `LanguageModelSchema.swift:131-134` already uses `.fragmentsAllowed`. Other Apple packages calling `JSONSerialization.data` (`Contacts.swift`, `ContactStore.swift`, `Motion.swift`, `AgentAttestation.swift`, `Calls.swift`, `BrowserAutomation.swift`, `BarcodeDetection.swift`, `Changes.swift`, `SpeechRecognition.swift`, `Browser.swift`) pass top-level containers rather than arbitrary handler values.

### Suggested fix and regression test
- **Apple (`Payments.swift`):**
  - Line 285: skip `NSNull`: `for key in ["total", "displayItems", "shippingOptions"] { if let v = update?[key], !(v is NSNull) { details[key] = v } }`.
  - Line 362: skip `NSNull` and serialize fragments safely (matching `ExactNativeModule.swift:203-206`): `if let data = value?["data"], !(data is NSNull), JSONSerialization.isValidJSONObject([data]), let text = try? JSONSerialization.data(withJSONObject: data, options: .fragmentsAllowed) { out["data"] = String(decoding: text, as: UTF8.self) }`.
- **Android (`Payments.kt`):**
  - Line 246: use `if (!u.isNull(k))` instead of `if (u.has(k))`.
  - Lines 252, 279, 281: add a null-safe string helper (`if (isNull(k)) null else optString(k).ifEmpty { null }`) for `u["error"]`, `shippingAddressErrors` entries, `retry["error"]`, and `retry["shippingAddress"]` entries.
  - Line 289: if `value != null && !value.isNull("data")`, JSON-encode `value.get("data")` (e.g., `JSONArray().put(d).toString().let { it.substring(1, it.length - 1) }` or `JSONObject.quote(d)` for `String`) instead of raw `it.toString()`.
- **Regression tests:** Add unit tests on Apple (Swift) and Android (Robolectric beside `packages/payments/test/CancelTest.kt`) covering: (a) `onpaymentauthorized` returning `{ result: "success", data: null }` and `{ result: "success", data: "order-123" }`, asserting no crash and `data` omitted / `"\"order-123\""`; (b) `onshippingaddresschange` returning `{ total: null, displayItems: null, shippingOptions: [...], error: null }`, asserting the prior total/displayItems remain unchanged and no error is attached; (c) `retry: { error: null }` falling back to `"Try another card."`.

### Siblings checked

packages/payments/web/index.js:32,35-37,138 (safe); runner/src/runner/modules/sheet.rs:533-536,593-596 (safe); host/web-js/sheets.js:181,198 (safe); host/apple/modules/ExactNativeModule.swift:199-206 (safe reference); packages/language-model/apple/LanguageModel.swift:419-429 & LanguageModelSchema.swift:131-134 (uses .fragmentsAllowed); other packages/*/apple JSONSerialization.data call sites pass containers
