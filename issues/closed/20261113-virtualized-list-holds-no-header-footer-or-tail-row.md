# A virtualized list holds nothing but its one `for`, so headers, footers and a streaming row become data

**Status:** Closed
**Resolution:** A vertical virtualized list holds fixed content before and after its keyed rows (LLP 1035.005.000 D11, built as LLP 1070 §14); the runs on a physical Apple device are in QUEUE.md.
**Systems:** contract/lower/src/collection.rs (`lower-collection-template`), collection runtime (LLP 1070), docs/recipes/chat-thread.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`list virtualized=true` takes exactly one direct `for` and nothing beside it.
`Lowerer` refuses a header, footer or tail beside the loop:

```
[lower-collection-template] a virtualized list initially supports exactly one direct `for`
```

The refusal forces headers, loading and end states into synthetic data rows.
A streaming reply concatenated to the transcript through a derive recomputes
that list on each token; that cost was reported, not measured.

## The failure

- A chat app (exact2 `5434f4325`, then `4d295cadc`): the streaming tail row
  beside the `for` was refused, and the tail was concatenated into the one
  list through a derive. `docs/recipes/chat-thread.md` shows a settled
  transcript only, and does not say how to show a reply while it streams.
- A port of an existing mobile app (diary on `94b8671f7`): a plain `scroll`
  with a `for` mounted every row and froze for 170 ms when a page arrived;
  `list virtualized=true` fixed that, but the feed's header and footer had to
  become synthetic rows. It asked for header and footer slots, and for a build
  hint when a `for` over a resource sits in a plain `scroll`.

## Fix

Select LLP 1035.005.000 D11 with these consumers: fixed nodes before and
after the `for` in a virtualized list, as a scrolling container on the web
holds them. Measure them as items in the height index, retaining authored
padding and the children's formatting context.

The approved chat recipe keeps a streaming reply as the keyed last data row
so its focus, selection and state survive completion. A fixed trail serves
loading and end notices.

## Implementation

The refusal now says what to write today and names `docs/recipes/long-lists.md`,
whose loading row is the pattern: a row of the `for`'s collection, appended by
a derive. The chat-thread recipe has no room under its size cap for the
streaming case. What remains is selecting LLP 1035.005.000 D11.

## Decision (2026-11-14)

LLP 1035.005.000 D11 stays unselected, amended 2026-11-14 from a blind panel
(`llp/reviews/panel-2026-11-14-diary-decisions.*`, D3). Two of three seats found
the runtime cost small (`concat` clones `Rc` items; the 65,536-step meter is far
above a chat screen) and the authoring cost real. When D11 is selected, its first
build changes only the compiler: fixed nodes before and after the `for` lower to
synthetic rows of the same collection with reserved keys. The trigger is a third
diary hitting this, not a performance number. This issue stays open as that
evidence.

## Decision (2026-11-15)

LLP 1035.005.000 D11 is selected and built as LLP 1070 §14's height-index
model, superseding the decision of 2026-11-14 above (Charlie, who had
approved the model on 2026-11-12, before that panel met). Fixed nodes are
always-mounted items in the height index under their own identities, not
synthetic data rows, so row counts, `aria-setsize`, `firstVisibleKey`, keyed
`scrollIntoView`, reorder and the reach events name data rows alone.

## Height-index build

The compiler admits lead and trail nodes around one direct keyed `for` on a
vertical list with `appearance="none"`. Cells lists, horizontal fixed content,
a second direct `for` and a virtualized list inside fixed content remain
refused by name.

The runner and hosts measure fixed items separately from authored padding.
Apple uses the shared kernel's occupied-flow measurement before pixel
snapping, reports both fixed identities even without mounted data rows, and
includes fixed regions in UIKit coverage and AppKit's prepared content.
Anchors retain fixed-region offsets; a followed end follows a growing trail.
Nested lists keep a visible data key with its signed offset, or a fixed
identity when no data shows, across their outer row's retirement.

The trail stays accessibility-hidden and inert until the geometric data window
reaches the last data row, then restores both authored values. Whole-list
copy on the wasm web target, the default JS web target and AppKit includes
fixed text and every logical data row when tall fixed content leaves no
mounted data rows. The JS target installs the selection controller after
paint and evaluates compiled text expressions for unmounted rows; compiled-app
clipboard tests cover both tall fixed sides, row scopes, selection exclusions
and nested collection boundaries before and after retirement. AppKit keeps
fixed-region and data endpoints when Select All becomes a range. The web observes layout
boxes beneath contents shells, so intrinsic image growth needs no app-state
commit. Display-hidden fixed children contribute no margins or extent.

Still owed on a physical Apple device: momentum and rubber-banding while a
followed trail grows, and VoiceOver navigation from the last data row into
the trail. `QUEUE.md` lists those runs.
