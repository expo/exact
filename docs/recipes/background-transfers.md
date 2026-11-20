# Recipe: transfers the system finishes

On iOS, macOS and Android, data code can start a transfer that survives suspension. On the web,
`backgroundFetchStatus().available` is false. Grant `net.fetch` for the
URL, `fs.read` for each upload file and `fs.write` for `saveTo`.

```ts
import { backgroundFetch } from '@exact/background-transfer';
await backgroundFetch({ id: `photo:${key}`, requests: [{ url, form: [
  { name: 'album', value: album }, { name: 'photo', from: 'app:/data/out/photo.jpg', contentType: 'image/jpeg' }
] }] });
```

Use `bodyFrom` for a raw file or `form` for up to 256 ordered parts. Both
default to POST. The host streams files into a private snapshot before
answering. For a form it sets Content-Type and the boundary. A file part
defaults to the source filename and `application/octet-stream`. Form
metadata is limited to 64 KiB. On Android this includes the URL and headers.

The root hears `backgroundfetchsuccess`, `backgroundfetchfail` or
`backgroundfetchabort` in a wake. Read each response at `records[index].path`.
With `saveTo` it belongs to the app. Otherwise the host keeps it at
`app:/data/transfers/<id>/<index>` (grant `fs.read app:/data`). The id is
percent-encoded except for ASCII letters, digits, `-` and `_`.
`retireBackgroundFetch` removes the kept files and frees the id. If Android
rejects retirement with `UnknownError` (for example, a locked secret store),
retry.

On Android, WorkManager waits for a network. Three system stops or five consecutive
network failures without a kept response end in `fetch-error`. A drive owns its transfers
even with `--bind background-transfer`. `--reset-storage` cancels its jobs and frees its ids.

A transfer started within five seconds of a press, key or notification tap while the
app shows is person-initiated, including one started by a timer. Android 14 and later run it as a
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
