# A virtualized list holds nothing but its one `for`, so headers, footers and a streaming row become data

**Status:** Open
**Systems:** contract/lower/src/collection.rs (`lower-collection-template`), collection runtime (LLP 1070), docs/recipes/chat-thread.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`list virtualized=true` takes exactly one direct `for` and nothing beside it.
`Lowerer` refuses any other child list in `contract/lower/src/collection.rs`
(the `let [Node::Each { body, .. }] = children else` arm, about line 166):

```
[lower-collection-template] a virtualized list initially supports exactly one direct `for`
```

Confirmed at `49ae6357` with a list holding a `for` and a `text` after
it. The limit is deliberate for now, and its remedy is recorded and not
selected: LLP 1035.005.000 D11, "Fixed rows around a virtualized `each`",
recommends fixed nodes before and after the `for`, designed under LLP 1070,
and sits in that LLP's stage 4 (runtime designs, each with its consumer),
which is unselected. These reports are new consumers for it.

The cost is that every row that is not a record has to be invented as one.
An app puts its header, footer, loading and end states into the list's data
and dispatches them back out in the row view. A chat app streaming a reply
cannot keep the reply in progress as a row of its own beside the settled
transcript; it derives `bubbles.concat([tail])`, so each token that changes
the tail recomputes a list as long as the transcript. That recomputation was
reported, not measured here.

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
holds them, laid out as rows that do not recycle. A streaming reply is then
a footer row reading its own state, and a token rewrites only that row.

Until then, the chat-thread recipe should show the streaming case as it must
be written today (the tail kept in its own state and appended by a derive)
and name D11 as the coming form, and the refusal could point at that recipe.

## Status (2026-11-13)

The refusal now says what to write today and names `docs/recipes/long-lists.md`,
whose loading row is the pattern: a row of the `for`'s collection, appended by
a derive. The chat-thread recipe has no room under its size cap for the
streaming case. What remains is selecting LLP 1035.005.000 D11.
