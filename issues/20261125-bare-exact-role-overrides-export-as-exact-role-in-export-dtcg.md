# Bare Exact role overrides export as -exact-<role> in export::dtcg and unconditionally fail export::reimport (sibling export_dials::marks drops pin tracking)

**Status:** Open
**Systems:** forma / DTCG export & reimport (CLI and forma-server)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Asymmetric pairs)

Verification: verified by reading.

At `9c0431fd5`, `forma/src/export.rs:709-715, 780-796` (`reimport_within`) resolves exported token names exclusively against `cur.design.tokens[i].name.value` (exact and ASCII-lowercased), omitting the canonical `cur.tokens[i].name` that `export::dtcg` exports (`forma/src/export.rs:364-366, 376-381`). In addition, `forma/src/export_dials.rs:324-341` (`marks`, also publicly exposed via `marks_of` at `:433` and called from `contract/cli/src/forma_import.rs:214, 253`) indexes only `cur.design.tokens[k].name.value`, dropping pin tracking when an `Op::SetToken` names a bare role override by its canonical `-exact-<role>` name (or vice versa).

### Reachable trigger
1. Create a `.forma` design containing any bare Exact role override (LLP 1095 D4), e.g.:
   ```forma
   forma t

   theme
     secondary-label = #666666
   ```
2. Export via `contract forma export` / `exact_forma::export::dtcg` (`contract/cli/src/forma_serve.rs:945`). `check.rs:165-173, 380-389` matches bare `secondary-label` through `self.roles.get("-exact-secondary-label")` (since `syntax.rs:288-290` `role_spelling` returns `None` for bare names) and `check.rs:204-211` produces `Checked { name: "-exact-secondary-label", role: Some("secondary-label") }`. `export::dtcg` (`export.rs:364-366, 376-381`) emits the token keyed by `"-exact-secondary-label"` with `$extensions.exact.overrides = "secondary-label"`.
3. Modify the `$value` of `"-exact-secondary-label"` in the exported DTCG JSON and re-import via `contract forma import dtcg` / `exact_forma::export::reimport` / `reimport_against` (`contract/cli/src/forma_import.rs:181-185`).
4. For the sibling defect, pass `Op::SetToken { name: "-exact-secondary-label", .. }` (which `edit::token_index` at `forma/src/edit.rs:1169-1206` accepts as an edit to bare `secondary-label`) into `exact_forma::import::marks_of` (e.g. via `forma_import::plan`'s `marks` path at `contract/cli/src/forma_import.rs:244-258`).

### Actual vs. expected
- **Actual:** In `reimport_within` (`forma/src/export.rs:709-715`), `declared_as` and `declared_any` contain only `"secondary-label"`. At `:780-796`, looking up `name = "-exact-secondary-label"` misses both maps even though `overrides` is `true`, so re-import unconditionally refuses the edit with `"not in the design any more: a token is added to the design, not by re-import"` (only unedited re-imports escape via the early `Planned::Unchanged` return at `:776-778`). In `export_dials::marks` (`forma/src/export_dials.rs:324-341`), `exact` and `folded` likewise contain only `"secondary-label"`, so `Op::SetToken` with `name = "-exact-secondary-label"` hits `else { continue; }` and silently omits `Pin` entries.
- **Expected:** Every token exported by `export::dtcg` that remains in the design must resolve back to its declaration in `export::reimport` / `reimport_against` (as CSS system roles already do via case-folding, e.g. `forma/tests/it/limits_counts.rs:352`), and `export_dials::marks` must resolve any role spelling accepted by `edit::token_index` (`forma/src/edit.rs:1165-1187`).

### Verification and siblings
Verified by reading the exact code trace at `9c0431fd5` (the prebuilt `target/debug/contract` binary predated the Forma rename). Existing tests (`forma/tests/it/limits_counts.rs:352`, `forma/tests/it/export_dials.rs:388`) only exercised CSS system roles (`accentcolor` → `AccentColor`, where case-folding alone matches) or edited a non-role token (`ink`) referencing a bare role (`background`), never editing the exported `-exact-<role>` override itself.
- **Sibling affected:** `forma/src/export_dials.rs:324-341` (`marks` / `marks_of`).
- **Siblings safe:** `forma/src/edit.rs:1169-1206` (`token_index`, `role_key`), `forma/src/edit/batch.rs:24-45` (`Index`), `forma/src/import/reach.rs:25-48` (`Index`), `forma/src/import/mod.rs:676-741` (`split_roles`), `forma/src/session.rs:541-552` (all resolve both bare and `-exact-` spellings), and `forma/src/export_batch.rs:122-163` (`blamed`, which receives `Op::SetToken` using `decl.name.value`).

### Suggested fix and regression test
- In `forma/src/export.rs:709-715`, also index `cur.tokens[i].name` (exact and ASCII-lowercased) into `declared_as` / `declared_any` (or resolve via `crate::edit::role_key` as `edit::batch::Index` does).
- In `forma/src/export_dials.rs:324-341`, resolve `Op::SetToken.name` using `cur.tokens[k].name` and `cur.tokens[k].role` / `crate::edit::role_key` in addition to `cur.design.tokens[k].name.value`.
- Add integration tests in `forma/tests/it/export.rs` and `forma/tests/it/export_dials.rs` that export a design with a bare Exact role override (`secondary-label = #666666` and `background = accent`), edit `"-exact-secondary-label"` / `"-exact-background"` in the DTCG JSON, verify `reimport` emits `Op::SetToken` for the bare name and closes on `apply_all`, and verify `marks_of` reports pins when `Op::SetToken` uses `"-exact-background"` on a bare `background` declaration.

### Siblings checked

forma/src/export_dials.rs:324-341 (affected); forma/src/edit.rs:1169-1206, forma/src/edit/batch.rs:24-45, forma/src/import/reach.rs:25-48, forma/src/import/mod.rs:676-741, forma/src/session.rs:541-552, forma/src/export_batch.rs:122-163 (all handle role spellings or use declared names safely)
