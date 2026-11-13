# A mixed-encoding colour reverse skips CSS shortening

**Status:** Open
**Systems:** motion
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A hover-off halfway through a hover-on shortens the reverse, on every platform, when both ends share an encoding. When `from.oklab` differs from `to.oklab`, the start replaces `reversing_adjusted_start` with `to_oklab()`. That sets the OKLab flag and changes the components.

Shortening runs only when `after` equals `running.reversing_adjusted_start`. `Value`'s equality includes `oklab`. The new target is the author colour, compared as the slot stored it, so a legacy colour does not equal the OKLab copy. The reverse then takes the full duration. A wide colour is `Value::oklab`. A legacy colour is `Value::rgba8`.

## Where

`motion/src/transition.rs`, the mixed-encoding arm of `Running::start`. `motion/src/engine.rs`, the reversing check. `motion/src/property.rs`, `to_oklab`.

## How to reproduce

Transition a colour from a legacy `rgb()` to a wide colour, or the reverse. Interrupt it halfway by returning to the start colour. The reverse runs for the full duration. The same interrupt between two legacy colours runs for about half.

## Direction

Compare the reversing start in one encoding, or store the author value beside the OKLab curve, and shorten when the new target is that start.
