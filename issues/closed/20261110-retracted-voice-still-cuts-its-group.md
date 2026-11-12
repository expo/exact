# A sequence voice taken back by an edit still cuts the sounding voice in its group

**Status:** Closed
**Resolution:** Retractions restore pending group cuts after commands and sequence expansion, retain restorable voices under record pressure, and update output ends before slot reuse.
**Systems:** runner/src/sound.rs, host/web-js/sounds.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A voice in a group ends at the first of its natural end, the start of the next voice in the group, and a `stopSounds` that covers it (LLP 1096 D4). `Sounds::play` applies the second when the later voice is issued: it moves the earlier voice's `ends` to the new voice's start and marks it `by: group` (`runner/src/sound.rs:627-633`, at `dd6f5dcd`).

A sequence issues its voices a second ahead. When an edit takes one back before it starts, `retract_seq` ends that voice at its own start as `by: edit` (`sound.rs:464-474`) and leaves the voice it had cut as it was. The earlier voice is then cut at the start of a voice that never sounds. LLP 1125 D3 says of an edit: "Voices already sounding play on."

## The failure

A 250 ms sample at beat 0 and a 100 ms sample at beat 0.1, both `group="hat"`, at 60 bpm, so the second starts at 100 ms. At 50 ms an action removes the second step:

```
derive steps = closed ? [hit(0, "assets/kick.wav"), hit(0.1, "assets/hat.wav")] : [hit(0, "assets/kick.wav")]
sequence s when true
  play([Track(beats=4, loops=0, offset=0, output="", steps=steps)], bpm=60)
```

Observed, from the voice table:

```
armed at 0:
    voice 1 assets/kick.wav at 0 ends 100 by group
    voice 2 assets/hat.wav at 100 ends 200 by end
edited at 50: the hat's step removed:
    voice 1 assets/kick.wav at 0 ends 100 by group
    voice 2 assets/hat.wav at 100 ends 100 by edit
    ops: [End { id: 2, at: 100.0 }]
```

Expected after the edit: voice 1 ends at 250 `by end`, since no voice of its group starts while it sounds. Instead it stops at 100 with nothing in its place, and `state.sounds` says a group cut it.

An edit to a slower tempo does the same. With `bpm=bpm` and the action setting `bpm = 30` at 50 ms, the second voice is issued again at 150 and the first keeps the old cut, leaving 50 ms of silence:

```
    voice 1 assets/kick.wav at 0 ends 100 by group
    voice 2 assets/hat.wav at 100 ends 100 by edit
    voice 3 assets/hat.wav at 150 ends 250 by end
```

## Verification

Temporary tests in `runner/src/transport/tests.rs` ran both edits through `Runner::act` and printed the lines above. The JS target does the same for the removed step: `host/web-js/sounds.js` `retract` (line 136), driven under Bun through `rt.js`'s `commit`, left the first voice `ends 100 by group` and the second `by edit`. The tempo edit was not run on the JS target, and no output played either.

## Direction

When a retraction takes a voice out, give each voice it cut the end it has without it: its natural end, or the start of the next voice of the group that still sounds. `SoundOp::End` is documented as only moving an end earlier (`sound.rs:66`), so the outputs need to accept a later one, or a new op.

## Related

`stopSounds` has no such case: it ends every live voice of the group. LLP 1125's declared limits for edits cover a track's `offset` and a followed clock, not a group's cut. No issue or QUEUE entry mentions groups or retraction.
