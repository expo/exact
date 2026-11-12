# Cargo refuses the Dioxus bench crate: it sits inside the root workspace without being a member

**Status:** Closed
**Resolution:** `bench/dioxus/heavy-dx/Cargo.toml` declares an empty `[workspace]`, as the egui and GPUI bench crates do, so Cargo runs in the crate and resolves its committed `Cargo.lock` with `--locked`.
**Systems:** bench/dioxus/heavy-dx, Cargo.toml
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`, `bench/dioxus/heavy-dx/Cargo.toml` is a package with no `[workspace]` table. It lies under the root workspace, which lists it neither in `members` (`Cargo.toml:2`) nor in `exclude` (`Cargo.toml:32`: `exclude = ["vendor", "snapback4", "apps/harmony/app"]`). Cargo walks up, finds the root manifest and refuses every command in the crate. Every other crate under `bench/` belongs to a workspace declared under `bench/`; the two other comparison crates declare an empty one (`bench/heavy-list/egui/Cargo.toml:7`, `bench/heavy-list/gpui/Cargo.toml:7`).

`bench/dioxus/web/build.sh:29` and `bench/dioxus/mac/build.sh:26` build the Dioxus arm with `(cd "$B/heavy-dx" && "$DX" build …)`. `dx` runs Cargo in that directory, so the Dioxus half of the "Building" section of `bench/dioxus/README.md` cannot run from this checkout.

## The failure

```sh
cd bench/dioxus/heavy-dx && cargo metadata --no-deps --format-version 1 --offline
```

Observed, exit 101:

```
error: current package believes it's in a workspace when it's not:
current:   …/bench/dioxus/heavy-dx/Cargo.toml
workspace: …/Cargo.toml

this may be fixable by adding `bench/dioxus/heavy-dx` to the `workspace.members` array of the manifest located at: …/Cargo.toml
Alternatively, to keep it out of the workspace, add the package to the `workspace.exclude` array, or add an empty `[workspace]` table to the package's manifest.
```

Expected: the package's metadata, as the same command prints in `bench/heavy-list/egui`.

## Verification

Ran the command above and `cargo locate-project --workspace` in the crate; both print the error. The same `cargo metadata` in `bench/heavy-list/egui` prints the package. Not run: `dx build`. The pinned dioxus-cli is not installed here (`PART=dioxus bench/dioxus/web/build.sh` stops at "no dx at …"), and it cannot get past the same Cargo error.

## Direction

Add an empty `[workspace]` table to `bench/dioxus/heavy-dx/Cargo.toml`, as the egui and GPUI crates have. Its committed `Cargo.lock` then becomes the lock Cargo reads, which is what `bench/dioxus/.gitignore:6-7` says it is for.

## Related

Nothing in `issues/` or `QUEUE.md` names `heavy-dx`.
