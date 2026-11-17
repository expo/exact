# Windows global shortcuts ignore the repeat option

**Status:** Closed
**Resolution:** The Windows global shortcut passes `windows.repeat` to the host, which registers a chord without `MOD_NOREPEAT` while any of its readers asks for repeat and registers it again as readers attach, change and leave.
**Systems:** @exact/global-shortcuts, Windows desktop hotkeys
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, an app that registers a shortcut with `windows.repeat: true`
still gets the default registration that suppresses keyboard repeat. This
breaks an explicitly requested repeating action, such as adjusting volume
while a global shortcut is held.

`packages/global-shortcuts/windows/exact.json:7` declares `repeat`, and
LLP 1133.025 D8 specifies that it omits `MOD_NOREPEAT`.
`packages/global-shortcuts/windows/lib.rs:37–43` reads only the keys and ID
before calling the host. `host/linux/src/desktop.rs:37–44` provides no
repeat argument, and `host/windows/src/desktop.rs:250` always ORs
`MOD_NOREPEAT` into the registration flags.

An isolated Rust probe compiled the actual package implementation against
a controlled desktop context that recorded registrations. Separate
instances called with `windows.repeat: false` and `true` forwarded identical
arguments and returned success:

```text
windows.repeat=false and windows.repeat=true forward identical registration: ("session", "volume", "Control+K", "global-shortcuts", "globalshortcut")
```

The probe is `/tmp/exact-review-20261112/shortcut-repeat-probe.rs`, compiled
with `rustc --edition 2021` and executed on this Mac. It verifies argument
loss in the actual adapter. No Windows keyboard hook or GUI was launched;
the unconditional platform flag is established by the host source above.

Forward the requested repeat policy to the host and apply it when
registering the hotkey. Since the host shares a registration by chord,
define how readers with different repeat policies coexist and retain
those policies when readers attach and leave. Validate both explicit
values and the omitted default through the package and host boundary.

Checked all open and closed filesystem issues and the 275-issue GitHub
snapshot for the repeat option, `MOD_NOREPEAT`, and repeating shortcuts.
[GitHub #195](https://github.com/expo/exact/issues/195) concerns PrintScreen
recognition and reservation; it does not cover this declared option.
The separate Dock count issue also loses a Windows option at the desktop
boundary, but concerns a different API and platform operation.
