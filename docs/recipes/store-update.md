# Recipe: a newer binary from the store

`@exact/store-update` says whether the App Store or Google Play has a newer
binary and opens it (LLP 1133.038), for what Exact's own signed updates
(LLP 1030) cannot carry: a new host, a native module, a raised OS floor.

1. **Declare** the App Store lookup's grant line, Android-only apps too:

   ```ts
   export const grants = 'net.fetch https://itunes.apple.com';
   ```

2. **Read the status.** Show nothing while `available` is false: the store
   did not make this install. It does not mean an update exists;
   `updateAvailability` says that.

3. **Offer the update by what the store said.**
   - `listed` (Apple): a row the person may ignore, with `apple.version`
     and an `openStorePage()` button. Never block on it: the store may not
     offer that version to this device.
   - `offered` (Play) with `android.flexibleAllowed`: `startStoreUpdate(appUpdateType="flexible")`
     from a button. Call `completeStoreUpdate()` from a Restart button once
     `android.installStatus` is `downloaded`, including on a later launch.
4. **Block only on Play's offer** (`offered` and `android.immediateAllowed`:
   `startStoreUpdate(appUpdateType="immediate")`), or on a minimum version
   your own server names, which is Apple's only gate.
5. **Develop the UI under the agent.** No store is asked, and the status
   is a store install with nothing newer. Lay the status the screen needs.
   Play's flow is a `store-update` ticket: `clock settle` stops at it and
   prints its `@N`; run the drive again with `"tap @N accepted"` (or
   `canceled`, `failed`) appended. The package then refuses as Play would
   against the laid status.

   ```sh
   bun exact.mjs agent android 'prefer extension store-update.storeUpdateStatus={"available":true,"updateAvailability":"offered","android":{"flexibleAllowed":true}}' "tap update" "clock settle"
   ```

```text
use storeUpdateStatus, openStorePage, StoreUpdateStatus from "@exact/store-update"
use startStoreUpdate, completeStoreUpdate, StoreUpdateResult from "@exact/store-update/android"

component App
  resource store = storeUpdateStatus() as shape StoreUpdateStatus else empty()
  mutation started as shape StoreUpdateResult
  action update
    send started = startStoreUpdate(appUpdateType="flexible")
  action restart
    completeStoreUpdate()
  action open
    openStorePage()
  view
    column
      match store.android
        case some(p)
          if store.updateAvailability == "offered" and p.flexibleAllowed
            button press=update testId="update"
              text "Update"
          if p.installStatus == "downloaded"
            button press=restart
              text "Restart to Update"
        case none
          text ""
      if store.updateAvailability == "listed"
        button press=open
          text "Update in the App Store"
```

A universal purchase's Mac build answers `unknown`: its one record is the
iOS binary's. Play's flows are untested against a real listing; try one by
internal app sharing with a higher `EXACT_BUILD_NUMBER`.
