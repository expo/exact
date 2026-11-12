# Calc length handles collapse into 16 engine-style hash buckets on 64-bit hosts

**Status:** Closed
**Resolution:** `engine_hash` hashes a calc length's whole handle, so distinct calc lengths fall in distinct buckets.
**Systems:** kernel/src/shared_style.rs, kernel/src/style/handles.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, `engine_hash` hashes each length with `tag()` and `value().to_bits()` (`kernel/src/shared_style.rs:430`). For a calc handle on a 64-bit host, those read bits 0–7 and 32–63; bits 8–31 disappear (`vendor/taffy/src/style/compact_length.rs:97`, `:109`). The pair handles advance by 16 (`kernel/src/style/handles.rs:37`, `:60`), leaving only 16 distinct hashes for the first 10,000 pairs.

`Interner::get` scans the entire bucket with equality (`kernel/src/shared_style.rs:54`). Distinct calc widths therefore still take quadratic comparisons, even though width is among the hashed rows.

## The failure

Create 10,000 otherwise identical styles with `width="calc(50% + 1px)"` through `width="calc(50% + 10000px)"`, convert each with `StyleProps::to_taffy`, and hash it. Observed: 16 hashes, 625 styles per bucket. Inserting these distinct styles requires 3,120,000 equality comparisons. Expected: the hash incorporates the full handle, distributing distinct lengths instead of discarding their distinguishing bits.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p exact-kernel --test review_findings -- --nocapture`. The test copied `engine_hash` unchanged from the current source to reach the private function and used the real `StyleProps::to_taffy` and `IdHasher`. It printed:

```
10000 calc widths: 16 hashes, 3120000 equality comparisons for distinct insertions
```

The comparison count is the sum of `n * (n - 1) / 2` over the observed buckets, not a timing measurement. The assertion that more than 5,000 hashes exist failed. The temporary test was removed.

## Direction

For `is_calc()` lengths, hash `calc_value()`'s complete handle with a calc discriminator. Keep the numeric representation for ordinary lengths.

## Related

`issues/closed/20261110-engine-style-hash-omits-insets-and-grid-placement.md` added omitted rows to this hash; it did not repair calc handle hashing. `issues/20261110-calc-handle-tables-never-shrink.md` concerns the separate pair-table lookup and lifetime.
