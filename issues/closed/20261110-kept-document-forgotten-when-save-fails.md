# Choosing a kept document again forgets it when the kept set cannot be written

**Status:** Closed
**Resolution:** A choice of a kept document whose save fails puts the list back as it was, so the record the set on disk holds stays kept.
**Systems:** data/src/documents/kept.rs (LLP 1069.010.000 D3, D4)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`kept::keep` handles a document the app already keeps by taking its record out of the list, updating it and putting it back first (`data/src/documents/kept.rs:405`, `:430`, at `dd6f5dcd`). When the save then fails it removes the first record (`:433`) on the reasoning "not written is not kept". That is right for a new record. For one that was already kept it removes a record the set on disk still holds, and nothing puts the old one back.

The list in memory is what later saves write. So after one failed save the document is missing from `recentDocuments()` for the rest of the launch, and the next choice that does save writes the set without it: the document is forgotten for good, though the person only reopened it.

The save fails when the app's support folder cannot be written, for example on a full volume.

## The failure

```rust
kept::open(owner, app, &root.join("set/kept.json"));
kept::restore(owner, &[]);
let doc = kept::keep(owner, &root.join("notes/a.md"), "bm", "readwrite").unwrap();
// the set's folder cannot be written
std::fs::set_permissions(root.join("set"), Permissions::from_mode(0o555)).unwrap();
let again = kept::keep(owner, &root.join("notes/a.md"), "bm2", "readwrite");
std::fs::set_permissions(root.join("set"), Permissions::from_mode(0o755)).unwrap();
kept::list(owner)
```

Observed: `again` is `None`, `kept.json` is unchanged, and `list` is `[]`. After `keep` of a second file succeeds, `kept.json` no longer names `a.md`. Expected: `list` is still `[doc:/1/a.md]`, and the set on disk keeps it.

## Verification

Ran it as a test in `data/src` on macOS (`cargo test -p exact-data --lib -- --nocapture`): the assertion on the list fails with `left: [], right: ["doc:/1/a.md"]`, and the test prints `after the next save, a.md is on disk: false`.

## Direction

On a failed save, put back what the list held: remove the new record only when there was none, and restore the old record at its old place otherwise, as `kept::forget` does (`data/src/documents/kept.rs:506`).

## Related

QUEUE's "Kept documents, what stage 2 left" lists drives not yet made, not this. No issue names it.
