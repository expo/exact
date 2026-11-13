# The agent cannot drag to dismiss the keyboard or film under real timing

**Status:** Closed
**Resolution:** A real drag under `--touch platform` may end over a visible software keyboard (refused otherwise), iOS `layout` reports a running scroll's presentation offset with its `target`, and film under `--timing platform` samples the wall clock, naming missed samples, with `reproducible: false`.
**Systems:** `scripts/agent.mjs` on iOS (LLP 1012, LLP 1079)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Excord's stage 1 transcript lane (`apps/excord/llp/0001-excord.rfc.md`)
**Date:** 2026-10-22
**Needs:** two decisions, one per ask. (1) How the agent drives the software keyboard: LLP
1080.000 says "the keyboard is outside LLP 1080", and this Mac's simulator shows no software
keyboard, so a real `tap … drag` under `--touch platform` (§11) has no keyboard to end over. A
ruling would extend LLP 1080.000 §11 with a keyboard the driver can raise and a drag that ends
over it. (2) Whether film runs on real time: LLP 1012.001.000 D2 refuses `screenshot … over …
every …` under `--timing platform`, because UIKit's motion is then not the clock's. Filming a
scroll as UIKit runs it needs D2 amended to say what a frame's `at` means on the wall clock and
that such a film is not reproducible, and LLP 1012's `layout` to say whether a scroller's offset
during a running scroll is its presentation or its target.

## Symptom

Two things Excord's §3.7 and §3.11 need a person and a phone for:

- A drag that ends over the software keyboard arrives as a synthetic scroll, not a
  touch, so UIKit's interactive keyboard dismissal (`keyboardDismissMode`
  `.interactive`, which the list has) cannot be driven.
- `"screenshot … over 600 every 50"` films on the agent's clock, which skips a
  scroll's motion; under real UIKit timing it cannot film, and `layout` during a
  running scroll reports where it is headed, not where it is.
