# Recipe: over-the-air updates

An installed iOS, macOS, Android or Linux app takes signed updates to its
Contract, assets and `app.ts`. A new grant or native code needs a new binary
([store update](store-update.md)). In full:
[reference](../reference.md#over-the-air-updates).

1. **Make a key.** `bun <exact2>/scripts/deploy.mjs keygen myapp` writes
   `~/.config/exact/keys/myapp.pem` and prints the public half.
2. **Declare it in `app.json`:**

   ```json
   "deploy": {
     "store": { "ios": "A", "macos": "A", "android": "A" },
     "channels": { "prod": "https://updates.example.com" },
     "signing": { "key": "myapp", "keys": { "myapp": "<public half>" } },
     "activate": "app-decides"
   }
   ```

   - `store`: `"A"` links the update store into that platform's binary. A
     first build writes `"0"`, which links none, as an absent `android` does.
   - `channels`: the URL a binary reads at build time and checks. Serve the
     directory step 4 writes there, over HTTPS. `"appdrop"` is AppDrop's
     origin for the key, with no server.
   - `activate`: `next-launch` (the default) boots an update at the next
     cold launch, `app-decides` holds it until `deliveryActivate()`, and
     `immediate` applies it in the foreground.
   - `check.interval`: seconds between foreground checks (default 900).
     Coming to the foreground checks, at most once a minute.
3. **Build the binary people install.** A release build with a store needs
   `EXACT_UPDATE_GENESIS=1`, which starts a stream at sequence zero, or
   `EXACT_UPDATE_RECEIPT`, a published release to continue. A simulator and
   Android ([phone builds](mobile-build.md)):

   ```sh
   EXACT_UPDATE_GENESIS=1 bun exact.mjs ios --release
   EXACT_UPDATE_TRUST=production EXACT_UPDATE_GENESIS=1 bun exact.mjs android --release
   ```

4. **Commit, then publish.** A dry run prints what each stream takes;
   `--yes` publishes:

   ```sh
   bun exact.mjs deploy --dirty --origin ../origin
   bun exact.mjs deploy --dirty --origin ../origin --yes
   ```

   `--origin`, here and in step 5, is the directory the channel URL serves;
   omit it for `"appdrop"`. `--dirty` publishes the working tree, uncommitted
   edits too, and lists them; an app outside the checkout is refused without
   it.
5. **Send an edit now.** `bun exact.mjs push --origin ../origin` publishes
   the working tree in seconds (`--watch`: on each save) and refuses what
   needs a new binary.
6. **Offer it.** `staged` is true while a downloaded update waits:

```contract
shape Delivery
  staged: bool

component App
  resource delivery = exactDelivery() as shape Delivery
  action update
    deliveryActivate()
  view
    column
      if delivery.staged
        row testId="update-banner"
          text "An update is ready." flex=1
          button press=update testId="update-now"
            text "Update"
```

`deliveryActivate()` and `immediate` wait for composition, pointers and
gestures to end; module updates also wait for loading and requests. They apply
without retry. `state.replacement.pending` names the hold.
