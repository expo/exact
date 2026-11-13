# A non-ASCII URL can crash the terminal host during scheme validation

**Status:** Closed
**Resolution:** open_url compares each allowed scheme against the URL's leading bytes, so a URL whose characters straddle a scheme's length is refused instead of panicking.
**Systems:** host/terminal
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `Host::open_url` checks supported schemes with
`url[..p.len()].eq_ignore_ascii_case(p)` (`host/terminal/src/host.rs:1102-1105`).
The length guard checks bytes, but the slice also requires a UTF-8 character
boundary. An unsupported URL such as `💥💥` is eight bytes long and panics at
byte seven instead of returning `false`. Both `openURL` commands and followed
text links reach this validator, including links supplied by content.

The checkout's actual terminal binary reproduced the crash with this entry:

```contract
component App
  action open
    openURL("💥💥")
  view
    button press=open testId="open"
      text "Open"
```

```sh
target/debug/exact-terminal /tmp/exact-review-20261112/unicode-url.contract --size 20x4 --drive tap open print
```

It exits 101 with `end byte index 7 is not a char boundary` at
`host/terminal/src/host.rs:1105:48`. This is a headless drive; no URL opener or GUI
was launched. A failed scheme check should leave the app alive and report
`failed` when an `openURL` result target was provided.

Compare ASCII scheme bytes without slicing a Rust string at an unchecked
offset, or use a checked prefix. Cover unsupported Unicode strings whose code
points cross each scheme length, plus supported URLs containing Unicode after
their scheme.

Duplicate check: searched all open and closed filesystem issues and the cached
titles and bodies of all 275 GitHub issues for terminal URL, Unicode, opener
and panic reports. No existing report covers this validator.
