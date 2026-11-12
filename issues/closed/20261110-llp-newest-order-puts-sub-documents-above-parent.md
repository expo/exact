# The LLP reader's "newest" order lists each sub-document above its parent, indented under the document before it

**Status:** Closed
**Resolution:** "Newest" reverses the groups of documents that share a top-level number rather than the rows, so each parent comes first and is followed by its own sub-documents in number order.
**Systems:** apps/llp/data (lib.rs), apps/llp (app.contract)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

The sidebar's "newest" chip (`apps/llp/app.contract:96,105,195` at `dd6f5dcd`) asks `index(…, newest=true)`. `index_value` then reverses the whole list (`apps/llp/data/src/lib.rs:129-133`):

```rust
if newest {
    // Reversed whole, so a sub-document still follows its parent: the
    // nesting is the point of the order, not the direction.
    shown.reverse();
}
```

LLP 1033 records the same reasoning ("Reversal is of the whole list, which keeps a sub-document under its parent"). Reversing the whole list does the opposite. The index is in number order with sub-documents after their parent (`apps/llp/data/src/index.rs:190-194`), so reversed, every sub-document comes before its parent. Each row is still drawn at its own `depth` (`app.contract:252,255`), so the indented rows of one document sit directly under the previous top-level document.

## The failure

A corpus of `1030`, `1030.000`, `1030.001`, `1031`, `1031.000`.

- Observed with `newest`: `1031.000` (indented), `1031`, `1030.001` (indented), `1030.000` (indented), `1030`. The two `1030.*` rows read as children of `1031`, and `1031.000` has nothing above it.
- Expected, by the comment and LLP 1033: `1031`, `1031.000`, `1030`, `1030.000`, `1030.001`: parents newest first, each followed by its own sub-documents.

## Verification

Ran the `index` source over that corpus in a temporary directory through `answer`, `continuation` and `parse`:

```
newest first: [("1031.000", 1.0), ("1031", 0.0), ("1030.001", 1.0), ("1030.000", 1.0), ("1030", 0.0)]
```

Ran it over the project's own corpus: all 255 sub-documents are listed above their parent, and the list begins `1155`, `1154`, `1153.001`, `1153`, `1152.000`, `1152`, so `1153.001` is drawn indented under `1154`. Not run: the windowed reader on a host.

## Direction

Reverse the groups, not the rows: split `shown` where a `depth == 0` row starts a group, reverse the groups, and keep each group's order. Correct LLP 1033's sentence with it.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
