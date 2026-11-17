# The painting host that Windows and Android link is the crate named `exact-linux`

**Status:** Closed
**Resolution:** The shared crate is `exact-painted` at `host/painted`, and `exact-linux` at `host/linux` now holds only `display`, `input`, `vnc` and the Linux entry, depending on it as `exact-windows` does; `android.rs`, `android_hint.rs`, `com.rs` and `picker_windows.rs` stay in `exact-painted`: its own code calls each (`android_hint` through its `canvas_jni!` macro), and no crate below it is Android's or Windows' to move them to.
**Systems:** host/linux, host/windows, host/apple-painted, apps/*/android, apps/*/windows
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`exact-linux` (`host/linux`) describes itself as "The Linux host" (`host/linux/src/lib.rs:1`, `host/linux/Cargo.toml`). It is the painting host of every platform without a native presenter. `cargo metadata` shows 56 crates depending on it: 33 Linux app crates, 19 Android app crates, `host/windows`, one Windows app crate, `host/apple-painted` and `host/linux-update`.

What is specific to Linux is a small part of it. `lib.rs` gates three modules on `target_os = "linux"`: `display` (DRM/KMS), `input` (evdev) and `vnc`, about 3,100 of the crate's 121,000 non-test lines. The other platforms' code lives in the same crate behind conditions: across `host/linux/src`, 211 conditions in 48 files select Android, 126 in 37 files select Windows, 98 in 25 files select macOS, and 53 in 19 files select Linux. `android.rs`, `android_hint.rs`, `com.rs` (Windows COM) and `picker_windows.rs` are files of the "Linux host".

The name costs something each time it is read:

- A change under `host/linux` reads as a Linux change. It is as likely to be a change to the Windows or Android host, and a reviewer or `scripts/affected.mjs` reader has to know that.
- The crate's documentation states Linux's premise as the crate's: "no view tree of the platform's, because the platform has none to offer" (`lib.rs:12-13`). Windows and Android have one, and LLP 1115.004 decides case by case what to take from it.
- AGENTS.md needs a phrase to work around the name ("the presenter Linux, Windows and Android share") and lists `exact-linux` and `exact-windows` as separate hosts outside the gate, though one contains the other's presenter.

## Direction

Name the shared crate for what it is (the painting host) and leave `exact-linux` holding `display`, `input`, `vnc` and the Linux entry, depending on it as `exact-windows` already does. Move `android.rs`, `android_hint.rs`, `com.rs` and `picker_windows.rs` toward their platforms' crates where their callers allow. This is a rename and a move, with no behaviour change; `rules/RULES.md` ("Delete; don't deprecate") says to do it without an alias.

## Verification

Counted with `cargo metadata --no-deps` (dependents by manifest path) and `grep -rE 'target_os = "<os>"|cfg\(<os>\)' host/linux/src` at `793a059e`. No design document was found that chooses the current name over a shared crate: `grep -ril 'exact-painted\|rename.*exact-linux' llp/` finds only unrelated uses of "exact-painted".
