# Barcode browser test changes its expected pick ID on every poll

**Status:** Closed
**Resolution:** The suite counts each pick once before it waits, matches the path the web host gives a pick (`<launch>-<pick>.<ext>`), and fails a still whose pick never appears.
**Systems:** Barcode detection browser conformance tests
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `packages/barcode-detection/test/web.mjs:67` waits for
an uploaded image with a predicate that increments `picks` on every
poll:

```js
await until(page, 'path', t => t.startsWith(`app:/tmp/picked/${++picks}-`));
```

If the first read still shows the previous path while the app stores
the picked file, the next read expects pick 2 even though this is pick 1.
Every later poll advances the expectation again. The ten-second wait
cannot succeed, and the counter is then wrong for subsequent files too.
The caller ignores the timed-out result and continues, so the suite can
spend ten seconds per image while still reporting success.

Evaluated the exact predicate extracted from the source against a first
stale observation followed by the completed first pick:

```json
[
  {"path":"","matched":false,"picks":1},
  {"path":"app:/tmp/picked/1-isbn.png","matched":false,"picks":2},
  {"path":"app:/tmp/picked/1-isbn.png","matched":false,"picks":3}
]
```

The bounded probe is `/tmp/exact-review-20261112/barcode-pick-poll-probe.mjs`,
run with pinned Bun 1.4.2; it opens no browser. Increment the expected
pick ID once per file before polling, and fail explicitly if the expected
path never appears. Validate a pick that becomes visible after several
polls and the next pick in the same session.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The reported Android barcode implementation gap does
not concern this browser test.
