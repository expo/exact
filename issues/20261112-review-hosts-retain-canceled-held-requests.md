# Hosts retain canceled requests that a data source held

**Status:** Open
**Systems:** host/terminal, host/apple, host/linux, host/web
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, hosts save a `RequestOut` when a data source returns
`Dispatch::Held`, and remove it only when that source later releases the same
continuation token. A superseded resource request need never be released:
`DataSource::forgotten` explicitly tells sources to drop canceled calls
(`runner/src/runner/source.rs:835-849`). The hosts do not reconcile their
parked requests with that cancellation, leaving the request and its allocations
owned until the session ends.

The matching paths are:

- Terminal: insertion at `host/terminal/src/requests.rs:97-100`, removal only
  while processing `release_work` at lines 81-84.
- Apple: `host/apple/src/abi.rs:323-326,349-353`. Executor `forget` at line
  273 does not inspect the bridge's separate `parked` map.
- Linux: `host/linux/src/presenter.rs:995-1000` and
  `host/linux/src/presenter/pump.rs:73-78`. Executor retirement likewise does
  not own the presenter's parked map.
- Web Rust host: `host/web/src/host.rs:1100-1103` and
  `host/web/src/host/complete.rs:27-30`.

A probe linked the checkout's actual terminal library and booted a resource
whose argument changes on each action. Its source returns a fresh continuation
with `Dispatch::Held` and has no work to release after cancellation. Each
request carries a 65,537-byte body so an allocation counter can distinguish
its retention from the rest of the host. After 100 argument replacements:

```text
held=false: body allocations at boot=0, after 100 replacements=0
held=false: after dropping Host=0
held=true: body allocations at boot=1, after 100 replacements=101
held=true: after dropping Host=0
```

The control uses the same source and resource, with `Dispatch::Missing`
instead of `Held`. The probe is
`/tmp/exact-review-20261112/terminal-held-retention.rs`; it executes the
terminal host, while the other hosts above were checked at their insertion,
retirement and release paths. The body size makes the retention measurable;
even an empty continuation retains a map entry and the other request fields.
Repeated refreshes or changing query arguments can therefore grow a
long-running session without bound.

Reconcile parked work against runner ownership after commits, while preserving
requests whose effects the runner deliberately keeps. Add regression coverage
for cancellation, argument supersession and refresh before release, and a
control showing that a still-live held request is dispatched when released.

Duplicate check: searched all open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for held/parked request cancellation,
retention and leaks. No matching report was found.
