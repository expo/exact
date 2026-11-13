# The metronome stepper edits a tempo the readout is not showing

**Status:** Open
**Systems:** apps/metronome
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

While a MIDI clock is followed and the transport is not stopped, the number under the stepper is `transport.bpm`, the device's tempo. Slower and Faster write the separate `bpm` state. The number does not move. The sequence is armed with that `bpm`, and Share reads it too, so the card's caption can name a tempo the screen is not showing.

When the clock is dropped, the readout switches back to `bpm` and shows every step taken while it was hidden.

## Where

`apps/metronome/app.contract`, `tempo`, `slower`, `faster`, the `click` sequence's `bpm`, and the readout with `testId="bpm"`.

## How to reproduce

Follow a MIDI clock and start. Note the number. Press Faster several times. The number stays on the clock's tempo. Stop following. The number jumps by those steps. Share while following names the hidden tempo.

## Direction

While the readout is the followed clock, ignore the stepper, or show that the number is the clock's and that the stepper is editing the tempo used when the clock is dropped.
