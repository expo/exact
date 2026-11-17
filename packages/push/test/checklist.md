# Push through APNs, on a simulator and a device (LLP 1133.004 §9 P9)

What the drive (`drive.mjs`) cannot do: send through Apple's push service,
which needs the team's APNs auth key, and sign against a profile with Push
Notifications. A person with the key runs these. Each step names what to see;
a step that shows something else is a finding. The docs claim the APNs path
and the device only after the step that shows it has passed.

## What the account must have first

- An APNs auth key for the team (Certificates, Identifiers & Profiles ›
  Keys, with Apple Push Notifications service): the `.p8` file, its key id,
  and the team id.
- For the device steps, the App ID of the app being run registered with Push
  Notifications, and a development profile for it that includes the phone;
  then an Ad Hoc profile for it. An app with
  `modules/apple/notification-service/` also needs the exact App ID
  `<app id>.notifications`, and development and Ad Hoc profiles for it with
  the app's App Group and shared Keychain group.

## Sending one push

`TOKEN` is the device token the app shows (the fixture's `token` row, or the
journal); `TOPIC` is the app's bundle id; `HOST` is
`api.sandbox.push.apple.com` for a simulator and a development build,
`api.push.apple.com` for an Ad Hoc, TestFlight or App Store build.

```sh
KEY=AuthKey_XXXXXXXXXX.p8 KEY_ID=XXXXXXXXXX TEAM_ID=XXXXXXXXXX
JWT=$(KEY=$KEY KEY_ID=$KEY_ID TEAM_ID=$TEAM_ID bun -e '
  const pem = await Bun.file(process.env.KEY).text();
  const der = Buffer.from(pem.replace(/-----[^-]+-----|\s/g, ""), "base64");
  const key = await crypto.subtle.importKey("pkcs8", der, { name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
  const b64 = (x) => Buffer.from(typeof x === "string" ? x : new Uint8Array(x)).toString("base64url");
  const head = b64(JSON.stringify({ alg: "ES256", kid: process.env.KEY_ID }));
  const claims = b64(JSON.stringify({ iss: process.env.TEAM_ID, iat: Math.floor(Date.now() / 1000) }));
  const sig = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, key, Buffer.from(`${head}.${claims}`));
  console.log(`${head}.${claims}.${b64(sig)}`);')
curl -v --http2 "https://$HOST/3/device/$TOKEN" \
  -H "authorization: bearer $JWT" -H "apns-topic: $TOPIC" -H "apns-push-type: alert" \
  -d '{"aps":{"alert":{"title":"Delayed","body":"The 9:41 now leaves at 9:52"},"sound":"default"},"navigate":"/thread/42"}'
```

APNs answers `HTTP/2 200` with an `apns-id` header. `400 BadDeviceToken`
means the token belongs to the other environment (the other `HOST`).

## Simulator (an Apple silicon Mac)

1. `bun packages/push/test/drive.mjs --bound --sim <udid> --keep`: the drive
   prints the token's length and keeps the app. Read the token with
   `bun exact.mjs agent ios --bind push tree` in the kept app (the `token`
   row). Measured 2026-11-11 on an iPhone 17 Pro simulator, iOS 27.0: the
   simulator answered a token (80 bytes).
2. Send the push above to `api.sandbox.push.apple.com` with that token and the
   kept app's bundle id, the app in the background: the banner shows.
3. Tap it: the app opens `/thread/42` (the `got` row reads `/thread/42`).
4. Send it again with the app in front: the banner shows over the app; tap it,
   the same.

## iPhone

5. In an app that grants `device.notifications` and `device.push` and imports
   `@exact/push` (the fixture, made as `drive.mjs` makes it, with its bundle
   id registered as above): `bun exact.mjs ios --device --run`. The build signs
   `aps-environment` `development`; a profile without Push Notifications is
   refused before building, naming it.
6. Turn on push: the system's prompt; Allow; the token row shows a token
   (32 bytes on a phone, so 64 hex digits).
7. Send to `api.sandbox.push.apple.com`: the banner, on the Lock Screen too;
   tap it, `/thread/42`. Quit the app and tap another: the app launches at
   `/thread/42`.
8. Quit and reopen without pressing anything: the token row shows the same
   token (the relaunch only reads `pushSubscription()`).
9. Archive with the Ad Hoc profile (`--archive`), install it, turn on push:
   the token differs from step 6's; send to `api.push.apple.com`: the banner,
   and its tap opens the route.
10. In an app that also has a Live Activity taking pushes, the archive of step
    9 builds (registration beside the activity, under `device.push`), and the
    activity's update token and the device token both arrive.
11. With the fixture's notification service extension, send a visible alert
    with `"mutable-content":1`: its displayed title starts `[Exact] `. Lock the
    phone and repeat; the changed alert appears on the Lock Screen. Then make
    the download miss its deadline and confirm the best attempt appears: the
    title still starts `[Exact] ` and no late attachment is added.
12. Inspect the Ad Hoc archive from step 9: it contains
    `Payload/*.app/PlugIns/ExactNotificationService.appex`, the extension is
    distribution-signed before its app, its embedded profile names
    `<app id>.notifications`, and the archive passes the distribution service's
    validation.

## Android through Expo

Use the reporter's POCO X6 Pro (Android 14, HyperOS), the Firebase client for
its package id and the EAS project holding that Firebase project's FCM v1
credentials. Build and install the app from the [push recipe](../../../docs/recipes/push.md),
open it once, allow notifications, and register its Expo token with the backend.
Confirm the live subscription has both `android.installationId` and
`android.token`; the latter must be the FCM registration token. Its
`android.appId` must match the installed package, including any variant suffix.

1. Send the recipe's panic alert with the app in front, using the backend or
   [Expo's push tool](https://expo.dev/notifications). Set priority `high`,
   channel `urgencies`, and data `{ "id": "urgency-42", "navigate": "/urgencies" }`.
   Check Expo's ticket and receipt. The app's `push` handler runs once, posts
   on `urgencies` with the app sound and refreshes its list if configured.
2. Press Home and send again with a different id. The notification appears
   without bringing the activity forward. Tap it: `/urgencies` opens.
3. Press Home, then `adb shell am kill <package>`; confirm
   `adb shell pidof <package>` prints nothing. Send again. A headless session
   handles it and posts without opening the activity. Tap: `/urgencies` opens.
   A force-stop is a different platform state and is not this check.
4. Repeat with the screen locked, hear the custom sound, and inspect the
   channel in Settings. Check OEM battery/autostart restrictions separately.
5. Clear the app's Firebase registration through the project's test tooling,
   reopen, and check that a changed FCM token updates the live subscription,
   is exchanged with Expo again and receives the next send.

Without this project's configuration, JVM tests cover routing, expiry and
registration callbacks, and mocked fetch covers the exchange. They establish
neither Firebase registration nor delivery through Expo or HyperOS.
