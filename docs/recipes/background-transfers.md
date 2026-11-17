# Recipe: transfers the system finishes

`@exact/background-transfer` (iOS, macOS and Android; the web reads `available`
false and uses `fetch`) hands an upload or download to the system, which
finishes it after the app is suspended or terminated. Data code alone
starts one, since its headers may carry a credential; each `url`,
`bodyFrom` and `saveTo` must be granted (`net.fetch`, `fs.read`,
`fs.write`):

```ts
import { backgroundFetch, backgroundFetchRegistration, retireBackgroundFetch } from '@exact/background-transfer';
await backgroundFetch({ id: `photo:${key}`, requests: [{ url: uploadUrl, method: 'PUT', bodyFrom: 'app:/data/out/photo.jpg', headers: [{ name: 'Authorization', value: token }], saveTo: null }] });
```

The root hears `backgroundfetchsuccess`, `backgroundfetchfail` or
`backgroundfetchabort` with the registration, in a wake. Each response
body is a file: the request's `saveTo`, else the kept path
`app:/data/transfers/<id>/<index>` (grant `fs.read app:/data`), where the id
is one path segment, each byte outside ASCII letters, digits, `-` and `_`
percent-encoded: read `records[index].path`. Retire it
(`retireBackgroundFetch`) once what it carried is kept: until then the
event comes again at later launches. Retiring removes the kept files, and
the id can be started again once it has answered. On Android a retirement
that cannot remove them (a locked secret store) rejects with `UnknownError`
and the id stays taken: call it again.
`packages/background-transfer/test` is a whole example.

On Android a transfer runs as WorkManager work that waits for a network.
Three stops by the system or five network failures in a row, with no
response kept between them, end it as `fetch-error`. A drive's transfers
are its own, with `--bind background-transfer` too.

A start within five seconds of a person's press or key, or of their tap on
one of the app's notifications, while the app shows and may post
notifications, is that person's transfer (a timer's code inside those
seconds counts, as on the web). Android 14 and later run it as a
user-initiated job with a notification (a tap raises
`backgroundfetchclick`) and no time limit, Android 12 and 13 as foreground
work while the app shows. The system's stops never fail it: after three it
waits for the app to be opened. Every other start is a program's.

The event is raised in the running app. With none, Android starts the app
with no window and its data module loaded, the handler runs there, and the
job ends once what it stored is on disk. A link or a share that then opens
the app is its launch location. An ending the app does not retire is raised
this way three times at most, then at each launch that shows the app. A
force-stop aborts the unfinished transfers, and the abort is delivered at
the next launch. After a restart of the phone or an update, transfers carry
on unopened. None of this has run on a device:
`packages/background-transfer/test/checklist.md` has the drives.
