# On iOS, an `app-decides` update held as pending is not reported as staged after a relaunch, so the app never activates it

**Status:** Closed
**Resolution:** A launch selection preserves pending update facts; only activation candidates clear staged.
**Systems:** host/apple-update/src/store.rs, host/apple/src/host.rs, host/apple/Sources/ExactUpdates/Updates.swift, update/src/store.rs
**Severity:** P1
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-deploy-and-push-refuse-typescript-app-with-live-activities.md

## What is wrong

Under `deploy.activate: "app-decides"` (LLP 1030.000 D4) a checked bundle is
held in the record as `pending` until the app calls `deliveryActivate`, and
the app learns of it from `delivery.staged`. On iOS, after a relaunch, the
app's `delivery.staged` stays `false` although the record still holds the
pending entry, so an app that activates on `staged` never does.

Where it is lost is not confirmed. The store keeps the pending entry and
reports it: `update/tests/it/store.rs`
`app_decides_holds_a_checked_bundle_until_the_app_activates_it` reopens the
store and asserts `staged()` names the entry, and it passes at `49ae6357`. A
check of the same head answers `Current` (`admit_check`, the
`record.pending == envelope.digest` arm, `update/src/store.rs:881`), which
is correct and leaves `pending` alone. The places to look are between the store and the runner:

- At boot, `host/apple/src/host.rs:465-474` fills the delivery facts with
  `status_into` only when the session boots without a candidate; a session
  booted from `prepared_delivery` has `staged = false` forced
  (`host/apple-update/src/store.rs:605-606`), which is wrong whenever a
  pending entry exists beside a selected one.
- After a check, `Updates.completed` calls `refreshDelivery`
  (`Updates.swift:163`), whose facts come from the snapshot `run_check`
  refreshed. Whether a TypeScript app's runner hears the new facts, and
  whether `set_delivery` answers the `delivery` resource again when only
  `staged` changed, is unchecked.

## The failure

One agent, on exact2 `95f015045`, iOS, a TypeScript app with
`live_activities`: a pushed update downloaded and sat in `record.json` as
`pending` with `selected: null`; after relaunching, `delivery.staged` never
became true, the check answered `current` because the pending entry is the
head's digest, and `deliveryActivate` was never called.

## Fix

Reproduce with `apps/caltrain` (its update banner shows on `delivery.staged`) under
`app-decides` with a local origin: push, let it download, quit, relaunch, and
read `state.delivery`. Then make the facts a runner boots with and the facts
after a check both say `staged` whenever `Store::staged()` is `Some`, and add
an XCTest or `exact-apple` test for the relaunch.

## Verified behavior

`prepared_delivery` preserves the store's staged fact for a launch selection;
only an activation candidate clears it. A selected entry with another entry
pending reproduced the defect. The embedded-only case in the report passed
without this change.

The Apple adapter regression checks both launch paths, Caltrain's banner,
a repeated `current` check, activation, and a later staged-only change. All
5 adapter tests pass. An iOS Simulator drive against a local origin also
checks both relaunch paths before the check completes, then activates the
pending entry and sees `staged` clear.
