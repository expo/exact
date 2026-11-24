# Recipe: update a direct-distribution Mac app's binary

`exact deploy` and `exact push` already update Contract, TypeScript, assets
and signed Rust modules without a new binary ([over-the-air
updates](over-the-air-updates.md)). Swift, the host and binary inputs need one:
for that the app links [Sparkle](https://sparkle-project.org) as its own Swift.

1. **`app.json`**, then `bun exact.mjs lock`:

   ```json
   "host": { "macos": {
     "swiftPackages": [{ "url": "https://github.com/sparkle-project/Sparkle", "exactVersion": "2.10.1", "products": ["Sparkle"] }],
     "infoPlist": { "SUFeedURL": "https://example.com/appcast.xml", "SUPublicEDKey": "<generate_keys prints it>", "SUEnableAutomaticChecks": true }
   } }
   ```

   Without `SUEnableAutomaticChecks`, Sparkle first asks whether to check, and
   other checks wait for the answer.

2. **`macos/modules/Updates.swift`**. That folder replaces `modules/apple` on
   the Mac, so an app with a module there moves it here and adds this:

   ```swift
   import Sparkle

   // One per process: each window's session makes its own module.
   private let updater = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)

   final class Updates: ExactModule {
       override func call(_ request: [String: Any]) -> [String: Any] {
           let controller = updater // the first call starts Sparkle
           // Sparkle's window opens after this call returns, not inside it.
           if request["check"] as? Bool == true { DispatchQueue.main.async { controller.checkForUpdates(nil) } }
           return ["available": true]
       }
   }
   let exactModule: ExactModule.Type = Updates.self
   ```

3. **The menu item.** `-exact-menu="about"` puts it under About (LLP 1021).
   `updateState()` at launch starts the updater.

   ```contract
   shape Updates
     available: bool

   component Updater
     resource updates = updateState() as shape Updates else empty()
     mutation checked as shape Updates
     action checkForUpdates
       send checked = checkForUpdates()
     view
       main padding=16
         if updates.available
           button "Check for Updates…" -exact-menu="about" press=checkForUpdates
   ```

   In `app.ts`'s `sources`:

   ```ts
   updateState: (_a, _s, _t, native) => native?.available ? native.call({}) as Result<'updateState'> : { available: false },
   checkForUpdates: (_a, _s, _t, native) => native?.available ? native.call({ check: true }) as Result<'checkForUpdates'> : { available: false },
   ```

4. **Release.** Raise `EXACT_VERSION` and `EXACT_BUILD_NUMBER`, then
   `bun <exact2>/scripts/exact.mjs release <app dir>`: it embeds
   `Sparkle.framework` with its helpers, signs inside out, notarizes and
   staples. Put the zip from `target/dist/<app>/` in a folder, run Sparkle's
   `generate_appcast <folder>`, and upload the zip and the appcast where the
   feed says.

A development build runs Sparkle too; an `http://127.0.0.1` feed works there.
