# Boot-page events can replay on unrelated settled controls

**Status:** Closed
**Resolution:** The boot swap moves a queued press, edit or focus only to the settled element of the same view, which the server names in the swap script's `data-to`, and drops it when that view is gone or hears other events.
**Systems:** document activation, host/render/src/direct.rs, host/web-js/rt.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

A `paint=boot` page can replay a click on a different control when the answer
changes the page's structure. An edit can similarly copy a value into an
unrelated field. The handoff uses the element's position in the DOM as its
identity, although boot and settled trees can contain different conditional
branches or rows.

Reviewed at `f6be99af`. `host/web-js/capture.js` queues the actual clicked or
edited boot element. `boot_swap_js` in `host/render/src/direct.rs:309-318`
turns its path of child indexes into a path through the settled tree. If any
element occupies that position, it replaces the queued event's target with
that element. It checks neither the originating plan site nor the control
or handler identity. `host/web-js/rt.js:1431` later calls `.click()` on this
new target, or dispatches the queued edit.

For example, a loading branch's cancel button and a loaded branch's delete
button can occupy the same position. Clicking cancel before settlement then
invokes delete after settlement. Both branches being valid independent
renderings does not make their controls interchangeable.

## Evidence

Extracted and executed the actual `boot_swap_js` string on Bun 1.4.2, using
small DOM substitutes and the capture script's queue format. The boot root
contained a `cancel-loading` control; the settled root contained a
`delete-account` control at the same child index. After the script ran:

```json
{"originalTarget":"cancel-loading","replayedTarget":"delete-account","bootRemoved":true}
```

Calling the queued target's `.click()`, as activation does, invoked the
settled control's action. This verifies the published handoff script; it
is not an end-to-end browser drive of an app.

## Fix and validation

Carry stable logical identity through both documents and retarget only a
matching control with compatible event semantics. Discard queued events
whose originating control disappeared. Apply the same rule to copied
values and focus.

Exercise a delayed answer that replaces, inserts, removes and reorders
controls. An early click or edit must reach its surviving logical control
or be discarded, never reach a different control at the same position.

## Duplicate check

Checked open and closed filesystem issues and all 275 cached GitHub issues.
The closed `20261110-boot-document-left-in-page-on-kernel-fallback.md`
concerns removing the boot document on the fallback path. It does not cover
event identity during the swap.
