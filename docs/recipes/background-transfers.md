# Recipe: transfers the system finishes

`@exact/background-transfer` (iOS, macOS and Android; the web reads `available`
false and uses `fetch`) hands an upload or download to the system, which
finishes it after the app is suspended or terminated. Data code alone
starts one, since its headers may carry a credential; each `url`,
`bodyFrom` and `saveTo` must be granted (`net.fetch`, `fs.read`,
`fs.write`):

```ts
import { backgroundFetch } from '@exact/background-transfer';
await backgroundFetch({ id: `photo:${key}`, requests: [{ url: uploadUrl, method: 'PUT', bodyFrom: 'app:/data/out/photo.jpg', headers: [{ name: 'Authorization', value: token }], saveTo: null }] });
```

The root hears `backgroundfetchsuccess`, `backgroundfetchfail` or
`backgroundfetchabort` with the registration, in a wake. Each response
body is a file: the request's `saveTo`, else the kept path
`app:/data/transfers/<id>/<index>` (grant `fs.read app:/data`), where the id
is one path segment, each byte outside ASCII letters, digits, `-` and `_`
percent-encoded: read `records[index].path`. Retire it
(`retireBackgroundFetch`) once what it carried is kept. Retiring removes the kept files, and
the id can be started again once it has answered. On Android a retirement
that cannot remove them (a locked secret store) rejects with `UnknownError`
and the id stays taken: call it again.
`packages/background-transfer/test` is a whole example.

On Android a transfer runs as WorkManager work that waits for a network.
Three stops by the system or five network failures in a row, with no
response kept between them, end it as `fetch-error`. A drive's transfers
are its own, with `--bind background-transfer` too.

A start within five seconds of a person's press, key or notification tap,
while the app shows, is that person's transfer (a timer inside that window
counts, as on the web). Android 14 and later run it as a
user-initiated job with a notification (a tap raises
`backgroundfetchclick`) and no time limit, Android 12 and 13 as foreground
work started while the app shows. The system's stops never fail it: after three it
waits for the app to be opened. Every other start is a program's.

Neither requires `POST_NOTIFICATIONS` or `@exact/notifications`. When
permission is denied, Android 13 and later list the app in Task Manager,
outside the notification drawer
([UIDT](https://developer.android.com/develop/background-work/background-tasks/uidt),
[foreground work](https://developer.android.com/develop/ui/views/notifications/notification-permission)).

The event reaches the running app, or Android boots it with no window and
its data module loaded. The job lasts until the handler ends and its writes
are on disk. A link or share then opens at its launch location. An unretired
ending gets up to three background wakes, then each visible launch. A
force-stop aborts unfinished transfers; the next launch hears the abort.
Transfers recover after a reboot or update without opening the app.
`packages/background-transfer/test/checklist.md` lists the platform drives.
