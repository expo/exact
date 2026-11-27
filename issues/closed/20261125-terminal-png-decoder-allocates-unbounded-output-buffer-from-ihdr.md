# Terminal PNG decoder allocates unbounded output buffer from IHDR dimensions before validating limits

**Status:** Closed
**Resolution:** Validated PNG IHDR dimensions and output buffer size against pixel and byte limits before allocating, reused the RGBA decode buffer, added LRU eviction to Images::by_source, and bounded concurrent decode workers.
**Systems:** exact-terminal (terminal host / harness image loader)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, `decode()` in `host/terminal/src/image.rs:190-217` constructs a `png::Decoder::new(...)`, calls `decoder.read_info().ok()?` (`image.rs:193`), and immediately allocates `let mut buf = vec![0; reader.output_buffer_size()?];` (`image.rs:194`) before calling `reader.next_frame(&mut buf)` (`image.rs:195`) and without checking image dimensions or total decoded bytes. In `png 0.18.1` (`png-0.18.1/src/decoder/mod.rs:153-157, 215-227, 390-391, 668-680`), `read_info()` only reserves a single scanline (`width * bpp`) against `Limits::default().bytes` (64 MiB), while `Reader::output_buffer_size()` only checks `imglen <= isize::MAX as usize` and does not enforce `Limits::bytes` (as `png`'s own doc comment warns). Compounding this for valid images, `decode()` allocates a second full RGBA buffer on `png::ColorType::Rgba` (`pixels.to_vec()` at `image.rs:199`), `Images::by_source` (`image.rs:265, 356, 375`) retains every decoded `Decoded { rgba, png }` and error result indefinitely without eviction, and `Images::load` (`image.rs:335`) spawns an unbounded OS thread per distinct source.

**Triggering sequence:** Any `exact-terminal` app (including `apps/harness` via `/image <path>` at `apps/harness/data/src/commands.rs:164-206` -> `apps/harness/terminal.contract:497`, or any terminal entry rendering an `image` with a `data:image/png;base64,...` URL or local file) triggers `Host::after_commit` (`host/terminal/src/host.rs:546`) -> `Images::load` (`host/terminal/src/image.rs:309-344`) -> `fetch` (`:81`) -> `decode` (`:190`). A ~57–67 byte PNG (well below `MAX_DATA_URL_BYTES = 1 MiB` and `MAX_SOURCE_BYTES = 64 MiB`) containing an `IHDR` of `1_000_000 × 1_000_000` 8-bit RGBA with valid CRC and a 12-byte empty `IDAT` passes `read_info()` (single-row `buflen = 4_000_000 <= 64 MiB`), returns `Some(4_000_000_000_000)` from `output_buffer_size()`, and executes `vec![0; 4_000_000_000_000]`.

**Actual vs. expected:** The `vec!` allocation immediately aborts the entire terminal host process with an allocator OOM abort before any pixel data is decoded. Expected: as established by `host/terminal/src/image.rs:73-78` (`MAX_SOURCE_BYTES`, `MAX_DATA_URL_BYTES`), `image.rs:86, 457` (returning `None` -> `error` event on undecodable images), and sibling decoders (`host/painted/src/image/png.rs:155`, `host/windows/src/clipboard/image.rs:5, 33, 189-200`), `decode()` should validate dimensions and decoded buffer size before allocating and reject oversized images cleanly with an `error` event; `Images` should avoid doubling RGBA memory and bound cache retention and concurrent decode threads.

**Verification and sibling sweep:** Verified by reading `host/terminal/src/image.rs`, `host/terminal/src/host.rs:546`, `apps/harness/data/src/commands.rs:164-206`, and `png-0.18.1/src/decoder/mod.rs:125-244, 368-394, 668-680` plus `read_decoder.rs:80-107`. Checked all non-test PNG decode siblings across the repository:
- `host/painted/src/image/png.rs:155, 507-543`: safe (enforces `u64::from(width) * u64::from(height) <= exact_raster::MAX_SOURCE_PIXELS` in `inspect_png` and decodes row-by-row with `new_with_limits`).
- `host/windows/src/clipboard/image.rs:25-49, 72-73, 189-200`: safe (checks `MOST_PIXELS = 64_000_000` before `read_info` / `vec!` allocation and reuses `frame` for RGBA).
- `host/painted/src/image/jpeg.rs:47, 128`, `svg.rs:81`, `windows_bitmap.rs:30`, `host/web/storage-image.js:21, 317`: all enforce `MAX_SOURCE_PIXELS`.
- `host/terminal/src/image.rs` is the only unguarded production PNG decoder.

**Suggested fix and regression test:** Before allocating `buf` in `decode()`, check `width > 0 && height > 0`, `u64::from(width) * u64::from(height) <= MAX_SOURCE_PIXELS` (e.g. `64 * 1024 * 1024`, or a tighter terminal bound), and `output_buffer_size() <= MAX_DECODED_BYTES`; for `png::ColorType::Rgba`, truncate and reuse `buf` instead of `pixels.to_vec()`. Bound `Images::by_source` (e.g. evicting unreferenced entries or enforcing a total byte cap) and cap concurrent decode workers (e.g. a small fixed worker pool or semaphore). Add a unit test in `host/terminal/src/image.rs` passing a crafted ~57-byte `1_000_000 × 1_000_000` IHDR + empty IDAT PNG to `decode()` and `fetch("data:image/png;base64,...", None)` and asserting `None` / `Err` without panicking or aborting.

### Siblings checked

host/painted/src/image/png.rs (safe: checks exact_raster::MAX_SOURCE_PIXELS and decodes row-by-row); host/windows/src/clipboard/image.rs (safe: checks MOST_PIXELS=64_000_000 before allocation and reuses RGBA frame buffer); host/painted/src/image/{jpeg,svg,windows_bitmap}.rs and host/web/storage-image.js (all enforce MAX_SOURCE_PIXELS).
