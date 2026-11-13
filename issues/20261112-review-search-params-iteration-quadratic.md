# Native URLSearchParams iteration copies the complete query for every entry

**Status:** Open
**Systems:** js/src/pure.js, vendored Ibex URLSearchParams binding
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, iterating a native data source's `URLSearchParams` takes
quadratic work even when the query never changes. `js/src/pure.js:35`
requests a complete snapshot to validate the receiver; every `next()`
requests another at line 39, including the final completed step. For `n`
entries, one iteration materializes `n * (n + 2)` pairs.

Each snapshot also crosses into Rust, parses the serialized query, returns
the whole list as JSON, parses that JSON in JavaScript, and copies its
iterator into another array. The underlying snapshot implementation is
`vendor/ibex/crates/ibex2/src/bindings/url.js:252-256`. `entries()`, `keys()`,
`values()`, `forEach()`, spread, and `Object.fromEntries()` all take this
path. A modest query can cause substantial allocation and block a source
turn; browser builds use the browser's implementation instead.

A bounded probe evaluated both unchanged binding files. It supplied a
controlled query adapter in place of the Rust boundary and counted every
pair that boundary returned:

| Query entries | Snapshot calls | Pairs returned | With only Ibex's snapshot iterator |
| --- | --- | --- | --- |
| 100 | 102 | 10,200 | 100 |
| 500 | 502 | 251,000 | 500 |
| 1,000 | 1,002 | 1,002,000 | 1,000 |

The 1,000-entry case took about 200 ms on Bun 1.4.2 in this probe. That is
not a Hermes benchmark: the call and pair counts establish the complexity
independently of the stand-in's speed. The probe is
`/tmp/exact-review-20261112/search-params-scaling.mjs` and runs without a
host or network connection.

Keep live iteration semantics while avoiding complete snapshots when the
query has not changed. A mutation version or a shared parsed list can make
ordinary iteration linear. Account for mutations through both the params
object and its owning URL; replacing this with a frozen snapshot would
regress the live behavior the wrapper supplies. Validate unchanged-query
scaling alongside append, delete and URL-search replacement during iteration.

Checked open and closed filesystem issues, `QUEUE.md`, and all 275 cached
GitHub issues. Existing URLSearchParams reports concern request bodies and
query serialization, not iteration complexity.
