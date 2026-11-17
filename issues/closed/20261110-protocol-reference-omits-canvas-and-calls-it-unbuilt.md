# The protocol reference has no entry for the `canvas` request and still lists the canvas gateway and the relay as later stages

**Status:** Closed
**Resolution:** The protocol reference documents canvas requests, grants, scopes and refusals, and no longer lists the gateway, remote mode or relay as future work.
**Systems:** docs/forma-protocol.md, scripts/forma-server.mjs, scripts/forma-canvas.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`docs/forma-protocol.md` is "the reference for the design server's wire", and says "Each change to the wire changes this page in the same commit" (lines 3-7 at `dd6f5dcd`). Two parts of it are behind the wire.

**A request it never describes.** The wire serves `canvas`. It is in `METHODS` (`scripts/forma-server.mjs:121`), has the scope `read` (`scripts/forma-pairing.mjs:63`), and is dispatched at `forma-server.mjs:717` to the gateway's `mint` (`scripts/forma-canvas.mjs:396-412`). A paired client sends `{"method": "canvas"}` and is answered `{url, grant, expires}`: the canvas's entry URL and a single-use grant good for 30 s. The local token is refused with 400, a server with no canvas gateway or port answers 404, and a client past six grants a minute gets 429.

The page says none of this:

- "Requests" (line 483 on) has no `canvas` entry.
- The scope table's `read` row (line 469) does not name it.
- The status table gives 429 to pairing attempts only (line 277).
- The one mention of the request, at line 387 ("`canvas` through the relay's door mints from it"), gives no shape.

**A closing section that says it is not built.** "Not yet in the protocol" ends (lines 1893-1894):

> **Later stages:** stage 10's canvas gateway (C) and the design tool's remote mode (D, LLP 1122 T6); a hosted relay (stage 10c).

The same page documents the hosted relay as `forma/1.14` and `1.15` (lines 337-411), with the canvas through it, and the code for both is in `scripts/`.

## The failure

A client written from the reference cannot open the canvas of a server it paired with. Nothing says which request gives the entry URL or what comes back, and the page's last section says the gateway does not exist yet.

## Verification

At `dd6f5dcd`:

- `bun test scripts/forma-canvas.test.mjs -t "with stage 10 A"` passes (2 tests). In them a paired `read` client posts `{method: 'canvas'}` through the real dispatcher and gets 200 with `ok.url` and `ok.grant`.
- The wire asked `canvas` with the local token answers `400 {"error":{"message":"`canvas` is a paired client's: the local tool frames the dev loop itself"}}`.
- `grep -n '`canvas`' docs/forma-protocol.md` prints line 387 only.

## Direction

Add `canvas` to "Requests" (who may ask, the answer, its refusals and limits), to the scope table's `read` row and to the 429 row, and delete the gateway and the relay from "Not yet in the protocol".

## Related

No issue or QUEUE entry covers the reference's account of `canvas`. `issues/20261110-reference-misstates-the-turn-budget.md` is about `docs/reference.md`, another document.
