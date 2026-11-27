# mde() on the web JS target corrupts each-row DOM anchors (throwing NotFoundError/orphaning editors) and leaks wasm instances and listeners on every unmount

**Status:** Closed
**Resolution:** Update single-element each-row anchors when mde() swaps its textarea and register onEnd cleanup to destroy the editor and observers on unmount.
**Systems:** host/web-js Markdown editor integration (pieces.js, mount.js)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown / Copy-paste drift)

Verification: verified by reading.

1. **Exact location**
At `9c0431fd5`, `mde(e)` in `host/web-js/pieces.js:427-499` (emitted directly for `<textarea markup="markdown">` by `host/web-js/src/rows.rs:736-741`) replaces the `<textarea>` asynchronously via `host/web/markup-editor.js:848` (`textarea.replaceWith(el)`) without updating `each`-row anchors in `host/web-js/mount.js:303, 312, 333, 342, 485` or registering any `onEnd` cleanup.

2. **Reachable trigger**
- **Anchor corruption in `each`**: On the web JS target, render an `each` list whose row template has `<textarea markup="markdown">` as its sole top-level element. Wait for `./markup-editor.js` to load and install, then remove one row (leaving at least one other row, or with any sibling or `Leave` hook so the all-rows `parent.textContent = ''` fast path at `mount.js:361-373` does not run), insert a row before that row, or reorder rows.
- **Teardown leak**: Mount any `<textarea markup="markdown">` (inside `when`, `match`, `each`, or a route), wait for `./markup-editor.js` to install, then unmount it.

3. **Actual vs. expected result**
- **Actual**:
  1. `each` (`host/web-js/mount.js:333`, and SSR/lazy adoption at `:303, :312, :485`) stores `r.start = r.end = frag.firstChild` (the original `<textarea>` `e`). When `Editor` resolves, `host/web/markup-editor.js:848` runs `textarea.replaceWith(el)`, detaching `e` and placing `<div class="exact-markdown-editor" contenteditable>` (`n`) into `parent`. `pieces.js:439-442` calls only `replaced(e, n)` (`host/web-js/focus.js:30`, which updates only the `offered` WeakSet), `Ids.set(n, id)`, and `Views.set(id, n)`, leaving `r.start` and `r.end` (and `r.start.$rowKey` at `mount.js:342`) pointing to detached `e`.
     - Removing the row (`mount.js:376-384`) calls `leave(e)` on detached `e` and breaks at `n === r.end`, leaving `<div contenteditable>` `n` orphaned in the DOM forever.
     - Inserting a row before it (`mount.js:396, 424`) sets `anchor = r.start` (detached `e`) and calls `parent.insertBefore(batch, anchor)`, throwing `DOMException: NotFoundError`.
     - Reordering rows (`mount.js:412-424`) re-inserts detached `e` alongside orphaned `n` (or passes detached `e` to `Portals.moveRange`), or throws `NotFoundError` when moving another row before it.
  2. `mde(e)` (`pieces.js:427-499`) never calls `onEnd(...)`. Unmounting a Markdown editor never calls `n.exactMarkup?.destroy()`, leaking `wasm.mde_new()` (`markup-editor.js:168`, never freed via `wasm.mde_drop` at `:599`), `document`'s `selectionchange` listener (`:748`), `window`'s `pointerup`/`pointercancel` listeners (`:743-744`), and `markup-editor.js:189`'s internal `MutationObserver`, and never disconnects `pieces.js:483`'s attribute `MutationObserver`.
- **Expected**: Row boundaries stay valid when `mde(e)` swaps `e` for `n`, and unmounting destroys the editor and disconnects both observers (and aborts installation if unmounted before `Editor` resolves). Established by the wasm siblings (`host/web/wasm/commit.js:897`, `host/web/wasm/boot.js:412`), `host/web/markup-editor.js:592-600`, `host/web-js/editor.js:5-11`, and every other loader in `host/web-js/pieces.js`.

4. **Verification and sibling check**
Verified by reading `host/web-js/pieces.js:427-499`, `host/web-js/mount.js:303-498`, `host/web-js/focus.js:30`, `host/web-js/src/rows.rs:736-741`, and `host/web/markup-editor.js:164-190, 592-600, 742-748, 845-853`.
- Siblings checked: `host/web/wasm/commit.js:897` and `host/web/wasm/boot.js:412` both call `el.exactMarkup?.destroy()` (clean); `host/web-js/editor.js:5-11` (`startDecoration`) calls `onEnd(() => handle?.destroy())` and checks `e.isConnected && !own?.gone` (clean); every other piece in `host/web-js/pieces.js` (`gs:22`, `c2:96`, `nm:157`, `mo:290`, `onHeight:309`, `onTRelease:320`, `onReorder:331`, `onPan:349`, `wf:365`) registers `onEnd` (clean). Other `replaceWith` callers (`host/web/canvas2d-glue.js:67`, `host/web/gpu-glue.js:355`) replace the inner `<canvas data-surface>` child created by `host/web-js/tree.js:67-81` (`cv(e)`), not the row's top-level element.

5. **Suggested fix and regression test**
- In `mount.js` / `pieces.js`, either notify the enclosing `each` row when an element is replaced (e.g., record `e.$row = r` or a replacement callback so `replace(n)` updates `r.start`/`r.end` and transfers `$rowKey`/`$rowEnd`, or bracket rows containing a replaceable element with comment anchors).
- In `pieces.js:mde(e)`, capture `const own = Owner;` and `let editor = null, obs = null, dead = false;`, register `onEnd(() => { dead = true; obs?.disconnect(); editor?.exactMarkup?.destroy(); });`, guard installation with `if (dead || own?.gone || !e.isConnected) return;`, store `editor = n`, and disconnect any previous `obs` before creating a new `MutationObserver`.
- Add a browser test in `host/web/tests/markup-editor.test.mjs` (under the JS runtime path) that (a) mounts `<textarea markup="markdown">` as the sole top-level element of an `each` row, waits for `exactMarkup`, then inserts before it, reorders it, and removes it without `NotFoundError` or orphaned `.exact-markdown-editor` nodes, and (b) mounts and unmounts an editor inside a `when` branch and asserts `destroy()` runs and no stale `selectionchange`/`pointerup` handlers or observers remain.

### Siblings checked

host/web/wasm/commit.js:897 (clean), host/web/wasm/boot.js:412 (clean), host/web-js/editor.js:5-11 (clean), other loaders in host/web-js/pieces.js (gs, c2, nm, mo, onHeight, onTRelease, onReorder, onPan, wf — all register onEnd), host/web/canvas2d-glue.js:67 & host/web/gpu-glue.js:355 (replace inner <canvas data-surface> child from tree.js:67-81, not row root)
