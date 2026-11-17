# Recipe: change a remote notification before it appears

Put an Apple notification service extension's Swift in:

```text
modules/apple/notification-service/NotificationService.swift
```

Declare one `UNNotificationServiceExtension` subclass named `NotificationService`;
other Swift files are helpers. It has no Exact runtime and cannot run Contract or
TypeScript. The inventory is every non-dot `.swift` file plus top-level `PrivacyInfo.xcprivacy`,
excluding `target` subtrees. Links in the folder, `modules/apple` or `modules` and
non-UTF-8 names are refused. Entries use bytewise UTF-8 order; other visible files
are refused.

Grant both notification capabilities in `app.ts`:

```ts
export const grants = 'device.notifications purpose.notifications\ndevice.push';
```

The build derives the target. It writes
`PlugIns/ExactNotificationService.appex`, bundle id `<app id>.notifications`, and
principal class `ExactNotificationService.NotificationService`. It inherits the app's
version, minimum iOS, App Group and shared Keychain group. A device build needs a
profile for that App ID with the same groups. Put its required-reason declarations in
`modules/apple/notification-service/PrivacyInfo.xcprivacy`; the build validates and
merges them into the extension. Declare an approved reason for every required-reason
API it uses; Exact supplies none.

APNs invokes it only for a visible alert whose `aps` dictionary has
`"mutable-content": 1` and an alert title and body:

```json
{
  "aps": {
    "alert": { "title": "Encrypted", "body": "Open to read" },
    "mutable-content": 1
  },
  "navigate": "/messages/42"
}
```

Call the handler exactly once. On expiry, iOS shows the original notification;
`serviceExtensionTimeWillExpire()` should cancel work and return the best content.

An ordinary iOS or iPadOS build packages the folder. `--extensions none` or a list
without `notifications` excludes it; other Apple platforms refuse it. A source edit
starts a new binary cohort. Packaging verifies the recorded inventory into a private
snapshot; a tool already requiring a binary uses it to name the changed member. Otherwise
the folder is ordinary app source to freshness, watches, captures and scanners. An ignored
file may stale an iOS drive once; a service-only edit rebuilds web and stales a no-hatch
Windows build, as any edit under `modules/` does.

A tap opens only top-level `navigate` as an untrusted local location. Exact reads no
other key. Data-only wake, content extensions, critical-alert or filtering
entitlements, arrival events and Contract or TypeScript execution remain
unavailable. Live Activity push-to-start is `exactActivities()`'s, not the extension's
([contract grammar](../contract-grammar.md#live-activities-startactivity-updateactivity-endactivity)).
