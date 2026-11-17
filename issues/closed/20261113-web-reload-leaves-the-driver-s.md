# Web reload leaves the driver's contact and file drag set

**Status:** Closed
**Resolution:** A web reload clears the session's contact slot, and the Chrome carrier cancels a held file drag on the old document and forgets it.
**Systems:** scripts
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Interactive `reload`, and an authored web `reload`, call `carrier.reset`. That releases the browser finger and loads a new document. The session slot `s.contact` stays set, and Chrome's `dragged` stays set. The new document has neither gesture.

The next `tap` throws "a contact is already down". `tap up` reaches the carrier, which throws "no contact is down" before the session slot is cleared, so the drive cannot lift or press again. A later drop of the same files skips `dragEnter` because `dragged` still names them.

## Where

`scripts/agent-test.mjs`, `reloadWeb`. `scripts/agent.mjs`, the Chrome carrier `reset` and the `dragged` slot, and `tap` / `pointer`. `scripts/agent-playwright.mjs`, `reset`, which releases only the Playwright pointer. `scripts/agent-drag.mjs`, `fileDrag`.

## How to reproduce

On a web drive, `tap <target> down`, then `reload`, then `tap <target>`. The second tap is refused. `tap up` is refused and the contact slot remains. The same sequence with `tap <target> dragenter <file>` then `reload` then a drop of that file skips `dragEnter`.

## Direction

After a successful `carrier.reset`, set `s.contact` to null and `dragged` to null. Cancel a held file drag on the old document before the reload.
