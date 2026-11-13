# An Android payment timeout never answers the request

**Status:** Open
**Systems:** packages/payments
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

When `onpaymentauthorized` passes its budget, Android stores a fail result whose message is `authorize-timeout`, sets `letGo`, and tells Google the payment failed. `finish` treats `letGo` as a host let-go and returns without `s.reply`. The extension answers the runner only from that reply, so `showPaymentRequest` stays open.

Dismissing the sheet afterwards only logs. The stored fail is never delivered.

## Where

`packages/payments/android/Payments.kt`, `authorized` and `finish`. `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt`, the call that answers from the module reply.

## How to reproduce

Show a payment request whose `onpaymentauthorized` does not answer within the budget. The sheet is told the payment failed. The call stays pending after the sheet closes. No fail result arrives.

## Direction

On budget expiry, leave `letGo` false and reply the stored fail from `finish`, or reply it immediately. Use a separate flag if a later authorization must not call the app.
