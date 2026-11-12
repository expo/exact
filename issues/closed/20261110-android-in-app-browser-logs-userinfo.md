# On Android the in-app browser's log line names a URL's user name and password

**Status:** Closed
**Resolution:** On Android the in-app browser logs `scheme://host[:port]`, and a link no app opens is logged without its user name and password, so neither reaches logcat (the Kotlin was not compiled: this machine has no Android SDK or JDK).
**Systems:** packages/in-app-browser (Android)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Grok 4.7's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

`openBrowser` logs the page it opens by its origin: the package's header says "The journal
names the URL's origin" (`packages/in-app-browser/android/InAppBrowser.kt:9`). Android
builds that origin as `"${uri.scheme}://${uri.authority}"` (`InAppBrowser.kt:61`), and
`android.net.Uri.getAuthority` includes the user information
(<https://developer.android.com/reference/android/net/Uri#getAuthority()>). So
`openBrowser({ url: "https://alice:s3cret@example.com/private" })` logs
`in-app-browser: opened https://alice:s3cret@example.com` (`:98`; also `:64` under the
agent and `:68` when the embedder declines). `say` goes to `context.log`
(`:29`), which is `Log.i` (`host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:259`),
so the password is in logcat, which `adb logcat` reads on a device with USB debugging on.

Apple builds the origin from scheme, host and port
(`packages/in-app-browser/apple/InAppBrowser.swift:67-69`), and the web logs `URL.origin`
(`packages/in-app-browser/web/index.js:26`, `:30`); neither carries user information.

`issues/closed/20261006-auth-userinfo-disclosure.md` fixed the same leak in the runner's
auth summaries, and the draft
`20261110-transfer-url-keeps-userinfo.md` reports it for background transfers.

## To see it

Confirmed by reading code only; no Android device was run. On a device with Chrome, an app
that calls `openBrowser` with the URL above prints the password in
`adb logcat -s exact`.

## What a fix is

Log `scheme://host[:port]` on Android, as Apple does (`uri.scheme`, `uri.host`, and
`uri.port` when it is not -1). A drive that opens a URL with user information under the
agent and reads logcat (the module's lines go there, not to the journal: `QUEUE.md:186`)
keeps it so.
