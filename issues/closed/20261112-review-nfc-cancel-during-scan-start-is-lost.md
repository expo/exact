# Cancelling a web NFC scan while it starts leaves the reader scanning

**Status:** Closed
**Resolution:** The web `scan` registers its claim before the shared reader starts and answers from the claim, so a cancel or `abortScan` during the start ends it at once and the last claim ending aborts the start.
**Systems:** packages/nfc/web/index.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, `scan()` awaits `started()` before adding its request to
`scans` (`packages/nfc/web/index.js:180-195`). While the browser is starting
the reader or asking permission, neither `cancel(request)` nor `abortScan()`
can find the pending claim. They leave the abort controller untouched.

When startup finishes, the cancelled request is added as a new live claim.
The reader continues and raises `reading` events for work the app stopped.
A non-`once` scan can continue indefinitely. The shared-reader state machine
needs to represent pending claims as well as started ones.

## Reproduction

Ran the real web module under Bun 1.4.2 with an `NDEFReader` stand-in whose
`scan({ signal })` returns an explicitly held promise:

```js
const pending = impl.scan({ once: true }, { request: 7 });
impl.cancel(7);
await impl.abortScan();
resolveBrowserScan();
await Promise.resolve();
await Promise.resolve();
// signal.aborted === false; the original scan remains pending.
reader.onreading({ serialNumber: 'dummy', message: { records: [] } });
console.log(await pending);
// { reason: 'read', reading: 'r1', message: null }
```

The module emitted one root `reading` event after both cancellation paths
had run. No NFC hardware or browser permission prompt was used; the held
promise makes the startup/cancellation ordering deterministic.

## Direction and acceptance

Register the claim before awaiting reader startup, or keep a cancellation
generation that startup must recheck before installing it. If the last
pending or active claim ends, abort startup and release the reader. Test
cancel, abort-all, failed startup and a second live claimant during the
same pending startup; cancelled work must neither resume nor emit a tag.

## Duplicate check

Checked open and closed filesystem issues and the GitHub export on
2026-11-12 for NFC, scan startup, cancellation and `abortScan`. The existing
NFC report concerns iOS polling after a write; none covers web startup.
