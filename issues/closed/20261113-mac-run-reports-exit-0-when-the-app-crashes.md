# `build.mjs --run` on macOS exits 0 when the app it launched crashes

**Status:** Closed
**Resolution:** Mac --run propagates child status, reports termination signals and waits for macOS crash reports.
**Systems:** host/apple/build.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-mac-page-zoom-scrollers-recurse-until-stack-overflow.md, issues/20261113-mac-scroll-gutter-covers-recurse-until-stack-overflow.md

## What is wrong

For a macOS build, `--run` launches the app with `spawnSync` and ignores
the result (`host/apple/build.mjs:1340`):

```js
if (args.includes('--run')) spawnSync(bundlePath ? … : paths.binary, [], { stdio: 'inherit', env: … });
return;
```

`main` then returns normally, so the script exits 0 whatever the app did. An
app killed by `SIGSEGV` or `SIGABRT` gives `status: null, signal: 'SIGSEGV'`,
which nothing reads. `scripts/app-cli.mjs` passes the child's code through
(`:316-320`), so `bun exact.mjs mac --run` in an app's directory reports 0 as
well.

## The failure

Two agents, on exact2 `e5ac601e1`, macOS: their apps died of main-thread
stack overflows (the two related issues), and the terminal that ran the
app's `mac --run` showed exit 0 each time, which reads as a normal quit.

Confirmed at `49ae6357` by reading `build.mjs`; not run.

## Fix

Read the `spawnSync` result: on a signal, print that the app was killed by it
(and, where it exists, the path of the crash report under
`~/Library/Logs/DiagnosticReports`), and set `process.exitCode` to
`128 + signal number`, as `app-cli.mjs` does; on a non-zero status, pass the
status on.

Verified on macOS with a compiled program calling `abort()`: the run returns 134 and prints the existing DiagnosticReports path after macOS writes it.
