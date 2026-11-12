# The runner keeps a resource, a queued send and a host fact each in parts that every path must pair by hand

**Status:** Closed
**Resolution:** Runtime resources own their state outside the plan, queued sends end their own writes, and host facts restore on refusal.
**Systems:** runner/src/runner.rs, runner/src/runner/each.rs, runner/src/runner/queue.rs, runner/src/runner/writes.rs, runner/src/runner/delivery.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`Runner` (`runner/src/runner.rs:329-572`, at `dd6f5dcd`) has 125 fields, written by 124 `impl`
blocks in 116 files. Three of its records are not one value. Each is several fields that agree
only because every path that makes, moves or ends the record names every part. Five filed defects
are a path that named one part too few.

| Record | Its parts | The pairing | Filed misses |
| --- | --- | --- | --- |
| A resource | 13 tables indexed by resource row: `resources`, `resource_values`, `failed_args`, `failed_why`, `pending_res`, `overlays`, `store_readers`, `entropy_readers`, `watching`, `stale`, `awaiting`, `members.rows`, `ids.work.resources` | `fit_resources` (`each.rs:156-173`) resizes each by name. An element of a resource over a list is a row pushed onto the runner's own `plan.resources` (`each.rs:133-152`), so 23 walks over `plan.resources` now include elements and 4 take `declared_resources()` | `state-names-a-list-once-per-element`, `resource-over-a-list-rows-grow-without-bound`, `writes-over-a-list-never-retire` |
| A queued send | its place (`queue::Waiting`, `queue.rs:23-30`), its optimistic write (a `WriteRecord`), its invocation id | the write is found by position: "a queue's oldest" pending record (`writes.rs:107-122`). `drop_queued_of` (`queue.rs:320-334`) removes the place and leaves the record | `expired-wake-leaves-queued-sends-writes` |
| A host fact | the stored fact and the readers' answers | each setter compares, stores, finds readers by source name, recommits, and restores on a refusal: `viewport.rs:77-95`, `time.rs:47-80` and `:103-153`, `page.rs:20-52`, `surface_record.rs`, `root_font.rs`, `delivery.rs:127-139`, `activities.rs:96-109`. The last two have no restore | `refused-set-delivery-keeps-the-new-facts` |

All five are `issues/20261110-<name>.md`. Each gives a fix for its own path. The three about a
resource over a list call themselves "separate defects of the same feature", and one names the
cause ("members being rows of the plan"); their fixes are to one set of walks, one table and one
lookup.

## Verification

Counted the fields and `impl<D: DataSource> Runner<D>` blocks with `grep`; read `fit_resources`,
`add_member`, `in_flight_write`, `drop_queued_of` and the eight setters; counted the walks with
`grep -rn "self\.plan\.resources\.\(len\|iter\)()" runner/src/runner`, less tests and `each.rs`. Read the five
issues and checked each says what the table claims. Nothing here was run: the failures are the filed
issues', which their authors ran.

## Direction

Three changes, each to code that exists:

1. One `ResourceRow` struct for a resource's state in one `Vec`, so a new row is one push and a
   field cannot be left unsized. Keep an element's row out of `plan.resources`: a table of its own
   after the plan's rows (`settlement.rs:680-683` still calls the plan immutable), reached through
   one accessor that says whether elements are wanted.
2. Carry the write's id on `Waiting`, and end the record where the place is dropped.
3. One function that takes the field, the new value and the source name, and does compare, store,
   recommit and restore. The eight setters call it and keep only what is theirs (the page's
   suspend, the place's locale slot, the root font's kernel size).

## Related

The five issues above. `issues/20261110-datasource-wrappers-lose-calls-to-trait-defaults.md` is the
same shape in the data seam (a rule kept by each wrapper remembering it) and is already filed as a
root cause. No issue or QUEUE entry names these three structures.
