# Unbounded native decompression in DecompressionStream bypasses Hermes heap ceiling and OOM-kills the host process

**Status:** Closed
**Resolution:** Bound codec output and concurrent contexts to HostState max_heap, stream gzip header parsing in O(1) memory with a 128 KiB cap, check interrupts in codec loops, and release peak codec_out capacity after copy, on error, and on runtime teardown.
**Systems:** exact-js (Hermes native runtime, DecompressionStream / bytes door)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Unbounded growth)

Verification: verified by reading.

At `9c0431fd5`, the native `DecompressionStream` / `CompressionStream` door in `js/src/compression.rs`, `js/src/crypto.rs`, and `js/src/lib.rs` performs unbounded native Rust heap allocations outside Hermes's memory ceiling, allowing a small compressed payload to bypass Hermes heap isolation and OOM-abort or Jetsam-kill the entire host process:

1. **Unbounded native inflation (`js/src/compression.rs:153-198`):** `Decompressor::inflate_into` grows `HostState.codec_out: Vec<u8>` in a tight loop via `out.resize(at + STEP.max(input.len().saturating_mul(4)), 0)` (`:161`) until `MZStatus::StreamEnd` or input exhaustion, with **no upper bound on `out.len()`** and no turn-interrupt check. Because deflate compresses repeated bytes up to ~1032:1, a single 1 MiB chunk (within default `maxResponseBytes = 1048576` at `js/src/fetch.js:534` / `runner/src/request.rs:52`) or a few dozen KiB of compressed zeros inflates hundreds of MiB to multi-GiB in native Rust RAM *before* bytes door op 13 returns to Hermes (`js/src/streams.js:930-935, 946`). This completely bypasses Hermes's 64 MiB `DEFAULT_MAX_HEAP` (`js/src/lib.rs:146`, `js/src/shim.cc:307`) and recoverable OOM retirement (`js/src/shim.cc:289-294`, `js/src/retirement.rs:13-43`).
2. **Unbounded gzip header buffering and $O(N^2)$ scan (`js/src/compression.rs:92-101, 222-229`):** `Decompressor::write` appends chunks into `self.gzip.pending` with no size limit (`:93`) and re-runs `gzip_header(&self.gzip.pending)` from byte 10 on every chunk (`:94, 222-229`). An untrusted gzip stream with `FNAME` (`flags & 8`) or `FCOMMENT` (`flags & 16`) and no NUL terminator causes unbounded native memory growth and $O(N^2)$ scanning CPU.
3. **Pinned peak native capacity (`js/src/crypto.rs:177-179`, `js/src/lib.rs:860`, `js/src/compression.rs:99`):** After op 15 copies `state.codec_out` into JS, `source.clear()` (`crypto.rs:178`) never shrinks capacity. Even `Module::drop_runtime()` (`js/src/lib.rs:854-861`) only calls `self.host.codec_out.clear()` (`:860`), while `self.host: Box<HostState>` (`js/src/lib.rs:468`) survives OOM retirement and reload (`retirement.rs:30, 63-69`), permanently pinning peak inflated capacity in native RAM for the `Module`'s lifetime. Similarly `self.gzip.pending.clear()` (`compression.rs:99`) retains peak header capacity.
4. **Uncapped concurrent native codec contexts (`js/src/compression.rs:354-366`):** Op 12 inserts `Box<InflateState>` (~37 KiB) or `Box<CompressorOxide>` (~350 KiB) into `HostState.codecs` with no count or native-byte cap, letting a module allocate hundreds of MiB of native memory while staying far below Hermes's JS heap limit (and relying on `release_keys` GC sweeps in `js/src/shim.cc:108-118`).

### Reachable trigger
A native TypeScript data module pipes an untrusted HTTP response or WebSocket `zlib-stream` frame (the primary documented use case in `js/src/compression.rs:8-13`) into `new DecompressionStream("deflate" | "deflate-raw" | "gzip")`. Calling `writer.write(chunk)` invokes `js/src/streams.js:946` -> `__exact_bytes` (`js/src/shim.cc:260-279`) -> `bytes_door` op 13 (`js/src/crypto.rs:208-215`) -> `compression::door` (`js/src/compression.rs:387`) -> `Decompressor::write` / `inflate_into`.

### Actual vs. expected
- **Actual:** A small deflate/gzip bomb inflates unboundedly in native Rust memory and aborts/OOM-kills the host process (and even non-fatal spikes permanently pin `codec_out` capacity across turns and OOM reloads); an unterminated gzip `FNAME`/`FCOMMENT` header grows `gzip.pending` without bound in $O(N^2)$ time; creating many `CompressionStream`/`DecompressionStream` instances allocates unbounded native memory.
- **Expected:** Native allocations on behalf of a data module respect its `max_heap` / `capture_limit` memory boundary (`js/src/lib.rs:146`, `js/src/shim.cc:88, 178-181, 287-294`, `js/src/retirement.rs`), failing with a JS `TypeError` (or recoverable OOM retirement) instead of aborting the host process, and releasing large transient buffer capacities promptly.

### Verification and siblings checked
Verified by reading the end-to-end bytes door, compression codec, Hermes shim, and module lifecycle at `9c0431fd5`. Siblings checked:
- `js/src/compression.rs:296-324` (`Compressor::run`): also grows `out` with no ceiling and leaves `codec_out` capacity pinned after op 15.
- `js/src/resume.rs:349, 389` (`HostState.binary`): properly bounded upstream and releases backing memory after each message via `self.host.binary = Vec::new()` (though `lib.rs:861` also uses `.clear()`).
- `js/src/shim.cc:178-181` (`__exact_capture_string`): properly enforces `state->capture_limit` (`max_heap_bytes`).
- `js/src/crypto.rs:80-85` (`HostState.keys`): also has no count cap, though each `EcKey` is much smaller than `CompressorOxide`/`InflateState`.
- Web hosts (`host/web-js/render.mjs:368` and browsers): use the browser/Bun `DecompressionStream`, not `js/src/compression.rs`.

### Suggested fix and regression test
- Store the module's memory ceiling (e.g. `max_heap` / a strict per-chunk output ceiling derived from `self.max_heap`, updated in `set_max_heap`) on `HostState` and enforce it inside `Decompressor::inflate_into` and `Compressor::run` before each `out.resize(...)` (using `try_reserve` and returning `Err(...)` -> JS `TypeError` when exceeded).
- Bound `gzip.pending` to a small header limit (e.g. 64 KiB + `FEXTRA` limit) and track scan progress (or fail once `pending.len()` exceeds the cap) so gzip header parsing is $O(N)$ and bounded; drop/shrink `gzip.pending` after header completion.
- Cap concurrent entries in `HostState.codecs` (and optionally `keys`), and shrink `state.codec_out` (e.g. `if source.capacity() > STEP { *source = Vec::new(); }` or `shrink_to(STEP)`) after op 15, on error, and in `drop_runtime()`.
- Add unit tests in `js/src/compression.rs` and an integration test in `js/tests/` covering: (1) a small high-ratio deflate bomb rejected cleanly with `TypeError` instead of allocating unboundedly, (2) a chunked gzip header with unterminated `FNAME`/`FCOMMENT` rejected cleanly without quadratic growth, (3) `codec_out.capacity()` shrinking after a large inflation, and (4) hitting the concurrent codec cap cleanly.

### Siblings checked

js/src/compression.rs:296-324 (Compressor::run also lacks output ceiling); js/src/resume.rs:349,389 (HostState.binary properly freed via Vec::new()); js/src/shim.cc:178-181 (__exact_capture_string properly enforces capture_limit == max_heap_bytes); js/src/crypto.rs:80-85 (HostState.keys also uncapped in count, much smaller per entry); host/web-js/render.mjs:368 (web uses browser/Bun DecompressionStream).
