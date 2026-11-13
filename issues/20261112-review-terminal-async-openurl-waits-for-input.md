# A terminal openURL from a timer waits for unrelated keyboard input

**Status:** Open
**Systems:** host/terminal
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the interactive terminal loop drains `host.outbound` only after
decoding a stdin read (`host/terminal/src/term/unix.rs:479`). Timers, mount work,
data announcements and idle work can all run `openURL` and append to that queue,
but their loop iterations do not call `open_outbound`. A URL they requested
stays queued until a later key or mouse report arrives. Quitting before that
read loses it.

This affects actions that open a URL after asynchronous work, such as preparing
a document or getting an authentication URL. The command's optional result
target is already told `opened` by `host/terminal/src/host.rs:492-495,552-554`,
even while the system opener has not been called.

A PTY probe ran the checkout's actual terminal binary with a mount timer:

```contract
component App
  state fired = false
  action opened
    fired = true
    openURL("https://example.invalid/review")
  task later mount
    after(50, opened)
  view
    text `Timer fired ${fired}`
```

The probe waited for the terminal output proving the timer committed, then
observed two normal polling periods without input. An executable substituted
for `open` in the child's PATH recorded no invocation. Sending an unrelated
`x` key immediately invoked it with the queued URL:

```text
timer frame shown, opener before input: False
opener called after unrelated key: True
```

The probe is `/tmp/exact-review-20261112/terminal-openurl.py`. It starts and
cleans up its own PTY process. The substitute only writes a marker; no browser
or GUI was launched.

Drain outward commands after every turn that can commit, including timer and
announcement turns, and before processing a requested exit. Add a PTY
regression with an injected opener and a timer or asynchronous completion; it
must open without sending any input to the terminal.

Duplicate check: searched all open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for terminal opener, outbound and
asynchronous openURL reports. No matching report was found.
