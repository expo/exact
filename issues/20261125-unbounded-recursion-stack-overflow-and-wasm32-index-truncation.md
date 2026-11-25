# Unbounded recursion stack overflow and wasm32 index truncation in columnar save decoder

**Status:** Open
**Systems:** exact-game (data::columns, world::save, storage::raw)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, the columnar save decoder in `game/engine/src/data/columns.rs` has three untrusted-input validation and safety defects reachable from `World::load` (`game/engine/src/world.rs:1147-1152`) via `world/save.rs:48-63` (`read_slots`) and `storage/raw.rs:509-538` (`RawStorage::read_save`), as well as `Sim::restore` (`game/engine/src/sim/snapshot.rs:91-112` -> `World::validate_saved`):

1. **Unbounded recursion stack overflow (`columns.rs:471-500, 759-862`):** `slots()` validates a shape byte stream in a flat loop without tracking nesting depth, so `[STRUCT, FIELD, 0, SOME * N, NONE, END]` passes with `(kinds = [], blobs = 0)`. During replay, `Replay::begin_seq` (`:762`), `begin_struct` (`:780`), `variant` (`:800`), and `option` (`:815`) push onto `self.frames` with no depth cap, and `Replay::skip` (`:824-862`) recurses directly on `SEQ` (`:839`), `STRUCT` (`:845`), `VARIANT` (`:850`), and `SOME` (`:855`) with non-tail post-calls (`end_option()`, `end_variant()`, loop continuation).
2. **`wasm32` silent 64-bit index truncation (`columns.rs:437, 571, 604, 644`):** `Cursor::column` (`:437`: `table.get(*id as usize)`), `Rows::decode` (`:571`: `counts.get_mut(s as usize)`; `:604`: `column.get(id as usize)`), and `Rows::read` (`:644`: `self.row_shapes[self.next] as usize`) narrow `u64` varints using `as usize` rather than `usize::try_from`.
3. **Pre-budget infallible string allocation (`columns.rs:747-758`):** `Replay::string` calls `std::str::from_utf8(b)?.to_owned()` at `:753-755` *before* calling `self.outer.claim(s.len())?` at `:756` and without `try_reserve_exact`. Because `Cursor::blobs` (`:456-464`) supports RLE repetition (`header & 1 == 1`), one 64 MiB blob can be repeated across rows and trigger an infallible heap allocation before budget accounting rejects it.

### Triggering input
- **Stack overflow:** Pass a ~100 KB `EXGAME v6` save to `World::load` whose `state.slots` columnar payload has 1 row, 1 interned name `"x"`, and 1 shape `[STRUCT, FIELD, 0, SOME * 100_000, NONE, END]`. `Slot` (`world.rs:138-145`) derives `Data`, whose generated `read` (`game/derive/src/lib.rs:434`) calls `r.skip()` on unknown field `"x"`, recursing 100,000 frames deep in `Replay::skip()`.
- **`wasm32` truncation:** Set any dictionary index (`:437`), row shape index (`:571`, when `shapes.len() > 1`), or group row-dictionary index (`:604`) varint to `1u64 << 32`.
- **Pre-budget string allocation:** Encode a 64 MiB string blob RLE-repeated 33+ times; on the row that exceeds `MAX_LOAD_BYTES`, `Replay::string` allocates the full 64 MiB `String` before `outer.claim` returns `Err`.

### Actual vs. expected result
- **Actual:** `Replay::skip` overflows the native stack (`SIGSEGV`/abort) or 1 MiB `wasm32` stack (trap); on `wasm32`, `1u64 << 32` wraps to index `0` and silently reads entry `0` instead of returning `DataError` (breaking cross-platform determinism against 64-bit native hosts); `Replay::string` performs an unbudgeted infallible allocation before `claim`.
- **Expected:** Matching sibling decoders `game/engine/src/data/bin.rs:139, 231, 236-243, 305-309` ("nesting is bounded to protect untrusted saves", `frames.len() >= 256`, `usize::try_from`, `budget.text`), `game/engine/src/data/json.rs:274-286, 439-461`, and `game/engine/src/data/limits.rs:17-26` (`Budget::text`), `data::columns` must enforce a 256 nesting-depth cap, use `usize::try_from` for all `u64` indices, and claim budget + `try_reserve_exact` before copying strings.

### Verification & siblings checked
Verified by reading `columns.rs`, `world/save.rs`, `storage/raw.rs`, `world.rs`, `game/derive/src/lib.rs:429-437`, and all sibling decoders in `game/engine/src/data/`:
- `data/bin.rs` and `data/json.rs` both enforce `frames.len() >= 256`, use `usize::try_from` for index conversions, and claim budget with `try_reserve_exact` before allocating strings.
- Within `columns.rs`, other `as usize` casts at `:538` (`end <= limit as u64`) and `:705` (`slots()` bounds name indices `< names as u64`) are already range-checked against `usize` bounds; `:707-708` (`Replay::name`) claims before allocating (though it also uses `name.into()` instead of `try_reserve_exact`).

### Suggested fix & regression test
1. Add a `push(&mut self, frame: Frame)` helper on `Replay` enforcing `self.frames.len() >= 256` (returning `DataError`) and validate nesting depth/balance in `slots()`.
2. Replace `as usize` with `usize::try_from(...)` at `columns.rs:437, 571, 604` (which also keeps `:644` in-range).
3. In `Replay::string` (`:747-758`), validate UTF-8 first, call `self.outer.claim(s.len())?`, then allocate via `String::new()` + `try_reserve_exact(s.len()).map_err(super::limits::allocation)?` + `push_str(s)`.
4. Add unit tests in `columns.rs` for: (a) shape nesting depth > 256 refused cleanly via `World::load`/`Rows::decode`; (b) index `1u64 << 32` refused in mode-1 dictionary, `row_shapes`, and mode-1 row-dictionary columns; (c) RLE-repeated string rejected by budget before allocation.

### Siblings checked

game/engine/src/data/bin.rs (bounded 256 depth, usize::try_from, Budget::text), game/engine/src/data/json.rs (bounded 256 depth, claimed try_reserve_exact strings), game/engine/src/data/columns.rs:538 & 705 (already bounded by usize limits; Replay::name at :707 claims before name.into()).
