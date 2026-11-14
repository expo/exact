# Android cancel during authorization leaves the payment sheet

**Status:** Closed
**Resolution:** A call let go while its final handler runs is marked let go, so that handler finishes but a later authorization is refused without calling the app.
**Systems:** packages/payments
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`cancel` returns without setting `letGo` while `authorizing` is true. After that handler returns `fail`, `unknown`, or `retry`, Google's sheet is still up. A second `onPaymentAuthorized` runs the app handler for a request the runner already let go.

## Where

`packages/payments/android/Payments.kt`, `cancel` and `authorized`. `packages/payments/apple/Payments.swift`, the cancel that sets the let-go flag even while authorizing.

## How to reproduce

Let go of `showPaymentRequest` while `onpaymentauthorized` is still running. When the handler returns, the sheet stays. Authorizing again runs the handler a second time.

## Direction

Set `letGo` even while authorizing. Leave the sheet up, finish the in-flight handler, and refuse a later authorization without calling the app.
