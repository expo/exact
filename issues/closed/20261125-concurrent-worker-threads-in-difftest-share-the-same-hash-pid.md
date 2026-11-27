# Concurrent worker threads in difftest share the same <hash>.<pid>.tmp staging path in leanrun::run and leanrun::memo

**Status:** Closed
**Resolution:** Stage difftest leanrun cache writes to atomic <pid>-<serial>.tmp paths using a process-wide AtomicU64 counter and remove failed temp files.
**Systems:** semantics/difftest
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P3
**Related:** LLP 1184 (Check, then act)

Verification: verified by reading.

At `9c0431fd5`, both cache writers in `semantics/difftest/src/leanrun.rs` build their temporary staging path using only `std::process::id()`:

- `leanrun::memo()` (`semantics/difftest/src/leanrun.rs:214-217`):
  ```rust
  let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
  if std::fs::write(&tmp, lines.join("\n")).is_ok() {
      let _ = std::fs::rename(&tmp, &file);
  }
  ```
- `leanrun::run()` (`semantics/difftest/src/leanrun.rs:248-252`):
  ```rust
  // Whole or not at all: a reader never sees half an observation.
  let tmp = files[i].with_extension(format!("{}.tmp", std::process::id()));
  if std::fs::write(&tmp, lines.join("\n")).is_ok() {
      let _ = std::fs::rename(&tmp, &files[i]);
  }
  ```

### Triggering sequence

Run `difftest corpus`, `difftest quick`, `difftest explore`, `difftest lowering`, or `difftest expansion` with `DIFFTEST_CACHE=1` (as `scripts/async.mjs:383, 395` and `semantics/difftest/src/quick.rs:80` enable) and `DIFFTEST_JOBS > 1` (the default `available_parallelism / 2`), whenever two batches dispatched concurrently on separate worker threads miss the cache for the same target file `<hash>.txt`:

- In `check()` (`semantics/difftest/src/lib.rs:204-215`), worker threads call `leanrun::run()` concurrently. `cache_file()` (`leanrun.rs:179-192`) normalizes the batch-local definition name (`def <name> ` → `def _ `), so identical `(library, program, oracle, events)` in different batches hash to the same `<hash>.txt`. This occurs when `stored_variants()` (`lib.rs:668-691`) emits a case and its `(stored)` variant across a batch boundary and `Oracle::with_store()` (`oracle.rs:118-132`) skips every persisted slot (`rng.below(3) == 0`), producing an identical `oracle.text()` (`oracle.rs:253-292`), or whenever two corpus/explored cases with the same normalized program, oracle, and events land in different batches.
- In `lowering::run()` (`semantics/difftest/src/lowering.rs:277-279, 390-408`) and `expansion::check()` (`semantics/difftest/src/expansion.rs:210-255`), worker threads call `leanrun::run_module()` → `leanrun::memo()` concurrently. Neither skips empty batches, so two chunks whose cases are all skipped/refused produce byte-for-byte identical `module(&[])` text and collide on `<hash>.txt` and `<hash>.<pid>.tmp`.

### Actual vs. expected result

- **Actual:** Two threads in the same process open the exact same `<hash>.<pid>.tmp` path. If Thread A finishes `std::fs::write(&tmp, ...)` and Thread B then calls `std::fs::write(&tmp, ...)` (`O_WRONLY | O_CREAT | O_TRUNC`), Thread B truncates that shared inode to 0 bytes right before Thread A executes `std::fs::rename(&tmp, &file)`, publishing a 0-byte or partially written inode at `<hash>.txt`. Because `leanrun::memo()` (`leanrun.rs:209-211`) and `leanrun::run()` (`leanrun.rs:234-237`) treat any readable `<hash>.txt` (including an empty file) as a cache hit, any concurrent worker or parallel `difftest` process reading `<hash>.txt` receives empty or truncated lines—causing spurious `DIVERGE` or `expected N cases, got 0` failures (and persisting a corrupt cache file if Thread B fails mid-write). Failed renames also leave orphaned `.tmp` files behind.
- **Expected:** Per the invariant comment at `semantics/difftest/src/leanrun.rs:248` ("Whole or not at all: a reader never sees half an observation"), each cache write must stage to a temp path unique per write so concurrent worker threads in the same process never truncate or rename each other's staging file.

### Verification and siblings checked

Verified by reading `semantics/difftest/src/leanrun.rs:179-257`, `semantics/difftest/src/lib.rs:204-215, 668-691`, `semantics/difftest/src/oracle.rs:118-132, 253-292`, `semantics/difftest/src/lowering.rs:277-279, 390-408`, and `semantics/difftest/src/expansion.rs:210-255`.

Sibling sites checked:
- `host/ink/src/device/wgpu/cache.rs:33-39`: already solves this exact same-process concurrency bug using a `static NEXT: AtomicU64` serial (`format!("{}-{serial}.tmp", std::process::id())`) and removes `temp` if `rename` fails.
- `semantics/difftest/src/leanrun.rs:293` (`run_module_uncached`): includes the per-batch `{tag}-{b}` prefix alongside `pid`, so `.lean` files do not collide across threads.
- `semantics/difftest/src/js.rs:391-411`: isolates each worker chunk in its own subdirectory `root.join(b.to_string())`.
- `host/executor/src/process_groups.rs:78-95`: writes `json.<pid>.tmp` under an exclusive `file.lock()`.
- `scripts/async-inputs.mjs:238-243`: writes `${file}.${process.pid}.tmp` synchronously in single-threaded JS.

### Suggested fix and regression test

Extract a helper `write_cache(file: &Path, lines: &[String])` in `semantics/difftest/src/leanrun.rs` using a process-wide `static NEXT: AtomicU64 = AtomicU64::new(0);` (mirroring `host/ink/src/device/wgpu/cache.rs:34-39`) to name temp files `format!("{}-{serial}.tmp", std::process::id())`, and remove `tmp` if `std::fs::rename(&tmp, file)` fails. Add a unit test in `leanrun.rs` spawning multiple threads that concurrently call `memo()` on the same cache key and asserting every reader and the final cached file observe the complete, non-empty payload with no leftover `.tmp` files.

### Siblings checked

Checked host/ink/src/device/wgpu/cache.rs:33-39 (already uses pid-serial.tmp with AtomicU64 and removes tmp on rename failure), leanrun.rs:293 (per-batch tag prevents collision), js.rs:391-411 (per-chunk directory), host/executor/src/process_groups.rs:78-95 (protected by file.lock()), and scripts/async-inputs.mjs:238-243 (single-threaded synchronous JS).
