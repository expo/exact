# A second `web` for an app whose dev loop still runs starts beside it and does not name it

**Status:** Closed
**Resolution:** A second frontend for this app and target refuses with the existing loop's URL and PID.
**Systems:** scripts/app-cli.mjs (`spawned`), host/web/dev.mjs, host/web-js/dev.mjs, host/web/serve.mjs (`watchLauncher`), scripts/tamber-server.mjs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`bun exact.mjs web` is the app's `exact.mjs` (app-cli) running `host/web/dev.mjs` as a child.
The loop stops when its launcher is gone: app-cli passes its own pid as `EXACT_LAUNCHER_PID`
(`scripts/app-cli.mjs:309`), and the loop polls it every second (`watchLauncher`,
`host/web/serve.mjs:1485-1490`); the design server's `contract tamber serve` child ends at EOF
on its stdin. app-cli forwards SIGINT, SIGTERM and SIGHUP it receives (`:312-313`), but nothing
ends app-cli when its own parent ends. A wrapper that spawned `exact.mjs web` and is then
killed leaves app-cli, the loop and the design server running, holding the port and
`.exact/tamber.lock`.

A second `web` for the same app does not look for that loop. Without `--port` or `EXACT_PORT`
it moves to the next free port with only `port 8765 is in use; serving on <n>`
(`host/web/dev.mjs:57`); its design server finds the lock held and reports
`another tamber server writes <app> (its lock is .exact/tamber.lock)`
(`contract/cli/src/tamber_serve.rs:355`), and the loop goes on without one. Neither line says
that this app's own dev loop is still running, or where.

## The failure

macOS, at `91602a456`: SIGTERM to a wrapper that had spawned `exact.mjs web` left the loop's
inner process and its `contract tamber serve` child running and holding the port. A second
`web` then "started without error, logged only 'another tamber server writes' and served
nothing itself." The builder asked for a clear "port already served by pid N" failure. What the
second loop printed about its port was not in the report; at `49ae6357` it would move to
another port as above.

## Fix

Record a running loop per app (pid, URL) beside `.exact/tamber.lock`, and have a second `web`
for the same app refuse with "this app's dev loop already runs at <URL> (pid N): stop it or use
it". app-cli could also end when its parent does (poll `process.ppid` for a change, as the loop
polls its launcher), so a killed wrapper takes the loop with it.
