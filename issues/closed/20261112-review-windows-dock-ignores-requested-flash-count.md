# Windows Dock ignores the requested flash count

**Status:** Closed
**Resolution:** The Windows Dock reads `windows.count`, refuses one that is not a whole number of at least 1 with TypeError, and passes it through the desktop services to `FLASHWINFO.uCount`, where it takes the place of a critical request's flashing.
**Systems:** @exact/dock, Windows desktop attention
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `requestAttention({ windows: { count: 1 } })` and the same
call with `count: 7` produce the same desktop request. An author cannot
control the number of taskbar flashes through the declared Windows option.

`packages/dock/windows/exact.json:7` declares `count`; LLP 1133.025 D8 maps
it to `FLASHWINFO.uCount`. But `packages/dock/windows/lib.rs:22–23` reads
only `critical` and calls `attention(critical)`. The host interface at
`host/linux/src/desktop.rs:51` has no count argument, and
`host/windows/src/desktop.rs:422` always supplies zero for a critical
request or three otherwise.

An isolated Rust probe compiled the actual package implementation, with a
controlled desktop context recording its calls. Passing counts one and
seven both recorded `attention(false)` and returned success:

```text
windows.count=1 and windows.count=7 both call attention(false); no count reaches the desktop
```

The probe is `/tmp/exact-review-20261112/dock-count-probe.rs`; it was run
with `rustc --edition 2021` on this Mac. This verifies argument loss in the
actual adapter. No Windows GUI was launched; the resulting fixed flash
count is established by the host source above.

Carry the validated optional count through the desktop interface and use
it when constructing `FLASHWINFO`, with the declared precedence over
`critical`. Keep the existing default when the author supplies no count.
Validate explicit counts through the package and host boundary, including
an explicit count together with `critical: true`.

Checked all open and closed filesystem issues and the 275-issue GitHub
snapshot for `requestAttention`, `FlashWindowEx`, Dock count, and taskbar
flashing. No matching issue was found. The package's desktop checklist
tests the default three flashes only.
