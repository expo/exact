# Vendor inventories state facts the vendored files contradict

**Status:** Closed
**Resolution:** The inventories now match their copies: Taffy drops a stale 0.9.2 `.cargo_vcs_info.json`, lists patch 25 and marks every remaining patch (31 and 60 had been marked 30 and 59), Vello marks and lists the `scene.rs` lint change, wgpu-hal names `exact-ink` (its count was already 15 everywhere, since patch 15), and AccessKit drops `Cargo.toml.orig`.
**Systems:** vendor/taffy, vendor/vello, vendor/wgpu-hal, vendor/accesskit-windows
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Each `vendor/*/EXACT-PATCHES.md` is what a refresh from upstream is done against. At `dd6f5dcd` these statements in them are false. Each item names the file that shows it.

**Taffy** (`vendor/taffy/EXACT-PATCHES.md`)

- Line 5: "Its `.cargo_vcs_info.json` pins commit `77f385683c1d698c91a23a259f87fdddf26925fb`." `vendor/taffy/.cargo_vcs_info.json` says `8f30e394106af09dbf131d184e3cff25d20cc207`. One of the two is not the 0.14.0 archive's.
- Line 19 says packaging receipts are omitted; that file is one, and it is present.
- Line 328 tells the updater to "Reconcile every `EXACT PATCH` marker with this inventory". Of the patches line 6 lists as remaining, 21, 31, 32, 34, 60 and 63 have no `EXACT PATCH` marker in `vendor/taffy/src` (21 is named only as "(patch 21)" in two comments), so a reconciliation by marker passes without them.
- Patch 25 has a heading (line 812) with no "upstream" or "dropped" note, and the test it names exists (`kernel/tests/it/browser_position.rs:586`). It is not in line 6's list and has no marker. Headings skip 22 to 24.

**Vello** (`vendor/vello/EXACT-PATCHES.md:8`): "Every change is marked `EXACT:` in the source", and the four patches name `src/exact.rs`, `lib.rs`, `render.rs`, `shaders.rs` and `wgpu_engine.rs`. `src/scene.rs` also differs from the 0.10.0 archive: five `#[expect(` became `#[allow(` (lines 100, 150, 188, 312, 343), unmarked and unlisted.

**wgpu-hal** (`vendor/wgpu-hal/EXACT-PATCHES.md`)

- The title counts fourteen patches, the file lists fourteen, and the source carries markers for all of 1 to 14. Lines 80 and 235 give the number the build script reports as 12, line 235 calling it "the patch count", and it is 12 (`vendor/wgpu-hal/build.rs:5`; `gpu/build.rs:22` accepts 12 or more). The count `exact-gpu` checks stopped following the inventory at patch 12.
- Line 13: "Only `exact-gpu` links this version". `host/ink/Cargo.toml:33` asks for wgpu 30, and `Cargo.lock` resolves `exact-ink` to `wgpu 30.0.1`, whose `wgpu-hal` is this copy.

**AccessKit** (`vendor/accesskit-windows/EXACT-PATCHES.md:11-12`) lists `Cargo.toml.orig` among the files left out; `vendor/accesskit-windows/Cargo.toml.orig` is tracked.

The Parley and fontique inventories agree with their `.cargo_vcs_info.json` and `Cargo.toml`.

## Verification

- Read `vendor/taffy/.cargo_vcs_info.json`. Collected every `EXACT PATCH n` marker under `vendor/taffy/src` and compared the numbers with the inventory's headings and line 6's list; searched the same tree for the six numbers in any spelling.
- `diff -rq ~/.cargo/registry/src/index.crates.io-*/vello-0.10.0 vendor/vello` and a `diff` of `src/scene.rs`. Vello is the only vendored crate whose upstream version is in this machine's registry; the others were checked against their own markers and manifests.
- `grep -rhoE "EXACT \(EXACT-PATCHES\.md, [0-9, and]+\)" vendor/wgpu-hal`: markers for all of 1 to 14. Listed the `Cargo.lock` packages that depend on `wgpu 30.0.1`: `exact-gpu` and `exact-ink`.
- Listed the tracked files of `vendor/accesskit-windows`.

## Direction

Correct the sentences. Mark or list the `scene.rs` change. Give the six unmarked Taffy patches a marker, or say in "Updating this copy" that they have none, and put patch 25 in the list or say where it went. Make the wgpu-hal count 14 in `build.rs`, `gpu/build.rs` and the two sentences.

## Related

`issues/closed/20260901-taffy-patch-provenance-stale.md` fixed an earlier drift in the Taffy inventory. The Rapier note is a separate issue because it omits behaviour changes.
