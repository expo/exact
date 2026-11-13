# The Linux crate `exact new` generates runs no Rust hatches

**Status:** Closed
**Resolution:** The Linux and Windows bake writes `hatches.rs` (`exact_hatches()`, from `contract::native::write_desktop_hatches`), the generated crates pass it to `Composed`, and `exact hatch` writes `modules/linux/hatches.rs` for an app whose `app.json` names `linux` or `windows`.
**Systems:** scripts/app-shells.mjs (the `linux` shell), js/bake (the Linux bake), contract/cli/src/native.rs (`rust_hatch_entry`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from fixing `exact hatch` for apps `exact new` makes
**Date:** 2026-11-11

## What is wrong

An authored Linux crate runs the app's Rust hatches (`modules/linux/hatches.rs`) through its build
script, which calls `contract::native::rust_hatch_entry` (`apps/native-fixture/linux/build.rs`). The
crate `exact new` generates under `.exact/shells/linux` (`scripts/app-shells.mjs`, `kind === 'linux'`)
is built by `bake('linux')`, which writes no hatches entry, so a hatch the app declares never runs
on Linux, or on Windows, which shares its host. The Android shell includes them (`hatches.rs`, from
`js/bake/src/android.rs`).

`exact hatch` therefore writes no Linux stub for such an app and says so in its "to do" lines.

## What a fix is

Have the Linux (and Windows) shell's bake write the hatches entry as the authored crate's build
script does, then let `exact hatch` write `modules/linux/hatches.rs` when `app.json` names `linux`.
