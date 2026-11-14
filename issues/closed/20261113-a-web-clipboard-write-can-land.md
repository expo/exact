# A web clipboard write can land after a newer one

**Status:** Closed
**Resolution:** The browser is given one clipboard write at a time, each after the one before it settles, and a write superseded while it waits for its turn ends `AbortError` without reaching the browser.
**Systems:** packages/clipboard
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A write supersedes another only while a file is being read or an image decoded. `inFlight` is cleared in `finally` before `navigator.clipboard.write`. `cancel` and `destroy` only mark `inFlight`.

A second write that starts during the browser write does not abort the first. Both return success. Whichever `clipboard.write` settles last is what the clipboard holds, which can be the older item. `shownNumber` keeps the disclosed state of the newer write, so that state and the clipboard can disagree.

The closed file-read ordering bug is fixed. This wait is after that check. The agent path never calls the browser.

## Where

`packages/clipboard/web/index.js`, `write`, `cancel`, and `destroy`.

## How to reproduce

Write a large PNG, then write a different item while `navigator.clipboard.write` of the first is still pending. Both calls succeed. The clipboard can hold the first item after the second call has returned, while the disclosed last write is the second.

## Direction

Keep the write in `inFlight` until `clipboard.write` settles. After it settles, write only when this number is still latest.
