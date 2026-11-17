# Web gamepad labels an Xbox Wireless Controller as PlayStation

**Status:** Closed
**Resolution:** The web's name fallback checks each family's own name before Sony's generic "Wireless Controller", so "Xbox Wireless Controller" without a vendor id is Xbox with A, B, X and Y.
**Systems:** Gamepad web implementation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `packages/gamepad/web/index.js:30–35` identifies a
controller by a vendor ID when present, then by its name. The generic
`wireless controller` match returns PlayStation before the more specific
`xbox` match can run. An ID of `Xbox Wireless Controller` therefore gets
the PlayStation family. `shape()` at lines 71–75 exposes that family's
button labels, so an app asks an Xbox user to press Square or Cross
instead of X or A.

Imported the actual implementation with a controlled
`navigator.getGamepads()` returning one standard-mapped pad. The
production `getGamepads({})` answered:

```json
{"id":"Xbox Wireless Controller","family":"playstation","labels":["Cross","Circle","Square","Triangle"]}
{"id":"Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)","family":"xbox","labels":["A","B","X","Y"]}
```

The probe is `/tmp/exact-review-20261112/gamepad-family-probe.mjs`, run
with pinned Bun 1.4.2. It tests the implementation's handling of those
browser-provided IDs; it does not claim a physical-controller drive.
The existing web conformance fixture supplies vendor-qualified IDs, so
it misses the name fallback.

Check explicit family names before the ambiguous generic wireless name.
Validate both IDs above and the existing PlayStation and Nintendo
fallbacks through the public result, including their labels.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. `QUEUE.md` records the missing hardware drives, but
there is no existing issue for this deterministic classification error.
