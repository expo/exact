# On Android, `console.log` in app.ts never reaches logcat, and the documented switch for it is not passed through

**Status:** Open
**Systems:** runner/src/runner.rs (`log`), host/painted/src/canvas/journal.rs, host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt (`environment`), host/android/README.md, docs/reference.md
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261110-hermes-console-loses-errors-and-throws-on-cycles.md

## What is wrong

A data module's `console` lines go to the runner's journal (`Runner::log`,
`runner/src/runner.rs:1092-1100`), an in-memory ring, and to the platform's log only while
measuring. `docs/reference.md:1521` and `:1891-1893` say they reach the agent's `logs`, which is
all. On Android the host writes its own lines to logcat under the tag `exact`
(`host/painted/src/android.rs:58-64`); the journal is written there only when `EXACT_LOG_JOURNAL=1`
(`host/painted/src/canvas/journal.rs:1-13`).

`host/android/README.md:175-177` says the switch is set by launching with the extra
`x_log_journal=1`. The reader copies launch extras into the environment from a fixed list in a
development build (`ExactActivity.environment`, `ExactActivity.kt:638-645`), and `x_log_journal`
is not on it, so the documented switch does nothing. A release build reads no extras at all
(`:619-621`).

So on a phone, outside an agent drive, a builder has no way to see what app.ts logs, and the
builder guides do not say where the lines go.

## The failure

Two builders on macOS: at `bb8524503`, "console.log from the data module (app.ts) does not reach
logcat (only the host's 'exact' tag shows), so debugging a source on a phone meant drawing its
state in the view; the docs did not say where data-module logs go on Android"; and at
`b04926c78`, the same on a release build, while debugging an upload.

## Fix

In a development build, write the module's `console` lines (at least) to logcat by default,
under their own tag, as the host's lines are. Add `x_log_journal` to the reader's list, or drop
the README's instruction. Say in `docs/reference.md`'s `console` row where the lines go on each
host outside a drive, and how to read them (`adb logcat -s <tag>`). Whether a release build
should log is a separate choice; if not, say so there.
