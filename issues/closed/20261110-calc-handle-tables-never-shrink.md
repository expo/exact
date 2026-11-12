# The `calc()` handle tables keep every distinct value for the life of the process

**Status:** Closed
**Resolution:** Parsed and lowered calculations release their entries with their last owner; Apple batches own canonical expression text.
**Systems:** kernel/src/style/handles.rs, kernel/src/style/calc/mod.rs, host/apple/src/calc.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## The defect

Calculation interners retained every distinct value for the life of the process:

- `calc::TABLE` held parsed trees and `env()` fallback text;
- `CALC_PAIRS` held each percentage-plus-points value lowered to Taffy;
- `TREES` held each lowered tree, floor and added length.

The two lowering tables also searched from the start for an existing entry.
Hash indexes removed those scans, but still retained values after their owners
dropped. Apple's separate `LENGTHS` table retained every comparison sent to a
native context.

A changing binding exposed the leak:

```
view left=`calc(${played / duration * 100}% - 6px)`
```

The original measurement at `dd6f5dcd`, in a `host-dev` build, set 200,000
distinct trees without lowering them: resident memory grew by 40 MB, about
207 bytes per value. A binding changing at 60 Hz produces 216,000 values an
hour. This measurement did not drive a running app.

Unit reproductions left 16,000 pairs and 16,000 lowered trees after dropping
all styles, 200,000 parsed comparisons after dropping their dimensions, and
16,000 Apple comparisons after dropping their contexts.

## What now holds

Parsed calculations and lowered handles use weak hash indexes. Dimensions,
grid tracks and Taffy styles retain their calculation owners; the last owner
removes both index entries. Detached dimensions and retained layout styles
remain valid. Handle IDs are never reused, and a released handle resolves to
zero.

Apple batches carry expression text until a native context owns the parsed
calculation and its environment. There is no separate bridge table. Contexts
read canonical CSS with bounded parsing and check author limits on the tree,
so parentheses added by serialization do not invalidate a valid calculation.

Regression tests cover release after distinct style updates, detached
calculations, cloned layout styles, grid tracks and native contexts. The
reclamation tests use disjoint expression sets so parallel tests cannot keep
each other's values alive.
