# Native Headers iteration copies and sorts the whole list twice per entry

**Status:** Open
**Systems:** vendored Ibex Headers, native JavaScript data sources
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, iterating a native `Headers` object repeatedly allocates and
sorts its entire header list. `bindings/headers.js:227-228` asks separately
for each entry's name and value. Both operations in
`vendor/ibex/crates/ibex2/src/bindings/headers_ops.rs:94-108` call
`Headers::sorted_entries()`, which clones every name and value and sorts
them (`stdlib/fetch.rs:184-187`).

For `n` distinct headers, one iteration performs `2n` full-list sorts and
clones `2n²` pairs. `forEach`, all three iterator kinds, and construction
from another `Headers` object use this path. This adds quadratic
allocation to ordinary header copying; a response or application with
many headers can stall its source turn. Exact keeps Ibex's native
`Headers` implementation (`js/src/pure.js`).

Two bounded probes checked both halves. The unchanged JavaScript binding,
with a controlled host adapter, made exactly 200, 1,000 and 2,000 indexed
reads for 100, 500 and 1,000 entries. A second probe linked the compiled
Ibex library and performed the actual `sorted_entries().get(i).cloned()`
operation twice per entry. The 1,000-entry case copied two million pairs
and took about 86 ms in this checkout's debug build, excluding JavaScript
and boundary overhead. This is an operation-count reproduction, not a
release-host benchmark. The probe files are
`/tmp/exact-review-20261112/headers-iteration-scaling.mjs` and
`/tmp/exact-review-20261112/headers-iteration-scaling.rs`.

Provide indexed reads over a shared sorted representation, maintained or
invalidated when headers change. Preserve sorted, live iteration across
append, set and delete; a frozen iterator snapshot would change behavior.
Validate scaling and mutation during iteration together.

Checked open and closed filesystem issues, `QUEUE.md`, and all 275 cached
GitHub issues. The separate URLSearchParams iteration report concerns
query snapshots in Exact's JavaScript wrapper; this defect is in Ibex's
native header storage and needs its own fix.
