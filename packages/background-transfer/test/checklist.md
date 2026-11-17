# @exact/background-transfer: drives by hand

What the agent cannot drive: iOS relaunching a terminated app for its
transfers' events, a person's force-quit, and a phone before its first
unlock (LLP 1133.017 D10). Record the date, the OS version and what
happened beside each step when you run it.

Build the fixture as `test/apple.mjs` makes it, for the phone
(`bun host/apple/build.mjs --ios --device --run bgtfixture-apple` with
`EXACT_APP_DIR` set to the made app), against a copy of that suite's server
the phone can reach. The host's lines are in the system's log, subsystem
`dev.exact`, category `wakes`.

1. **In front.** Tap Upload u1. The server receives `hello u1` with the
   `Authorization` header; the app shows `u1 success success  201 kept u1`
   and the ids line empties.
2. **Terminated by the system.** Make the server hold `/upload`'s answer
   for 60 s. Tap Upload u2, go Home, and kill the process from Xcode's
   debugger before the answer. See the server's `/heard` for u2 within
   seconds of the answer, with no launch by hand: iOS relaunched the app,
   the host booted the session with no view, and the handler ran.
3. **A person's force-quit.** As 2, but swipe the app away in the app
   switcher. Nothing is heard while it stays closed. Open it: the app
   shows `u2 abort failure aborted`.
4. **Before the first unlock.** As 2, then restart the phone before the
   answer and leave it locked. Record whether anything is heard before the
   first unlock (D10's staging is not built, so it should not be), and what
   the app shows when opened after it.
5. **A large download.** Serve a file of several hundred megabytes at
   `/file`, tap Download d1 and lock the phone. Record how long it took and
   whether `saveTo` holds the whole file.

## Android

These checks need a phone or an emulator that boots. The JVM tests run the
lifecycle against the adapter and Robolectric's schedulers; they do not show
what Android's scheduler, WorkManager or the Keystore do. Build the fixture
above with `bun scripts/agent-android.mjs build <app> --apk` and
`EXACT_APP_DIR` set to its directory, against a server the phone can reach.
Record the Android version beside each step.

A start within five seconds of a press, key or tap on one of the app's
notifications, while the app shows, is a person's transfer: a user-initiated
job on Android 14 and later, foreground work on
Android 12 and 13. Steps 1 to 5 below start theirs from a press unless they
say a timer does; run each both ways. For a person's transfer also check its
notification, its progress, the tap that raises `backgroundfetchclick`, that
it outlasts ten minutes, that a start a timer makes six seconds after a
press runs as plain work with no notification, and that a start made from a
`backgroundfetchclick` or `notificationclick` handler, more than five seconds
after the last touch in the app, is a person's.

Repeat the tap-started upload on API 33, 34 and 35 with `POST_NOTIFICATIONS`
denied (`adb shell pm revoke <bundle-id> android.permission.POST_NOTIFICATIONS`),
and in an app that does not import `@exact/notifications` or declare that
permission. It must still use a user-initiated job on API 34 and later, or a
foreground worker on API 33, and finish. Record the scheduler's result,
where Android shows the transfer notice (notification drawer or Task Manager),
and that a timer's start outside the activation window remains ordinary work.

2026-11-17, AOSP ARM64 emulators, Android 13, 14 and 15 (API 33, 34 and 35):
a standalone platform probe completed a 16 MiB upload with HTTP 201, both
with permission denied and with it absent from the manifest. On API 34 and
35, `JobScheduler.schedule` returned `RESULT_SUCCESS`, the job reported
`isUserInitiatedJob` true and `setNotification` succeeded. On API 33,
WorkManager's worker promoted to a `dataSync` foreground service and
completed. All six runs showed the app in Active apps / Task Manager, with
no notification drawer entry. These runs checked the platform APIs, without
Exact's host or JNI; they do not cover the fixture drives below.

1. **In front.** Upload, download, a refused origin, a directory as `saveTo`
   and an abort. The upload sent is the file as it was at the start, though
   the source is edited afterwards. A second start of an id that is not
   retired is a `TypeError`. A kept body is at
   `app:/data/transfers/<id>/<index>`. Retiring removes the kept bodies and
   leaves `saveTo` files; the id starts again once the retirement has
   answered.
2. **A large upload's copy.** Start an upload of several hundred megabytes
   and go Home during the copy: the app stays responsive, and an abort during
   the copy answers at once. A start whose `bodyFrom` is missing rejects, and
   its id is free again.
3. **Stops and failures.** Hold the server's answer past WorkManager's ten
   minutes three times in a row: the transfer ends `fetch-error`, not
   `aborted`. Drop the network during a transfer: it runs again when the
   network returns, and ends `fetch-error` at the fifth failure with no
   response kept between them.
4. **Deaths.** Kill the process during a large upload, restart the phone
   during a download and install an update, each time leaving the app closed:
   the transfer finishes. A request cut short starts again from byte zero,
   and the requests already answered are not sent again. Kill during the
   upload's copy and during a response's placement: the copy is made again
   and the response is placed, with no body reported lost.
5. **Force-stop.** Force-stop from Settings during a transfer, then open the
   app: it shows the abort. Swiping the app away, an update and a restart
   are not taken for a force-stop. Run this on API 35 or later, where the
   package reads `ApplicationStartInfo.wasForceStopped`, and on an earlier
   one, where it reads a cancelled `PendingIntent`.
6. **Endings.** Finish a transfer with the app closed (swiped away, then its
   process killed with `adb shell am kill`): the server hears the handler's
   note with no window shown, and the transfer's notification or work stays
   until then. Open the app: the event has run, and what its handler stored
   through a data module's call is there. Open it instead from a link or a
   share within seconds of the note: it opens at that route, and Back from
   it goes where Back goes after a cold start at the same link. Left unretired, it comes three times in all without the app,
   then at each launch until it is retired. Open the app while the handler
   runs: it shows as it was left, and the event is not raised twice. Finish
   one with the app running behind another: the event is heard there. Retire, start the same id again, and see that nothing of
   the first transfer reaches the second.
7. **Locked.** A transfer under a locked screen, a restart before the first
   unlock, missing Keystore keys and a corrupt secret file. No credential is
   ever read from plaintext. Staging before the first unlock is not built.
   A retirement while the secret store cannot be opened rejects with
   `UnknownError`, the id cannot be started, and the same retirement after
   the unlock frees it.
8. **A drive.** With the app holding an unfinished transfer, drive it with
   `--storage <name> --bind background-transfer`: the drive lists no ids,
   cannot abort or retire the app's, and may start the same id. End the
   drive and open the app: its transfer is as it was.
