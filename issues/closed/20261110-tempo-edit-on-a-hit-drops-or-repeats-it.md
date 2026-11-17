# A tempo edit committed at a hit's time can drop that hit or play it twice

**Status:** Closed
**Resolution:** After a tempo edit the fill issues exactly the steps the edit took back (their time under the segment before it at or after the edit), none earlier than the edit, in the runner and in `transport.js`.
**Systems:** runner/src/transport.rs, runner/src/sound.rs, host/web-js/transport.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

An edit at `t` takes back the sequence's voices whose start is at or after `t` (`runner/src/sound.rs:467`, at `dd6f5dcd`). A new `bpm` then starts a segment at `t` whose beat is `seg.beat_at(t)` (`runner/src/transport.rs:935-941`), and the fill issues the hits whose time under that segment is at or after `t` (`transport.rs:438`).

What is taken back is decided by `Segment::time` and what is issued again by `Segment::beat_at` (`transport.rs:131-138`). In floating point the two are not inverses: `beat_at(time(b))` can be one step above or below `b`.

- A hit whose time is `t`, where `beat_at(t)` comes out above its beat: the voice is taken back, the hit's time under the new segment is below `t`, and it never plays.
- A hit whose time comes out just below `t`, where `beat_at(t)` is its beat: the voice has started and stays, and the hit is issued again at `t`.

## The failure

One track of 16 beats with one click, and an action that sets `bpm`, run at the time shown:

```
sequence s when true
  play([Track(beats=16, loops=0, offset=0, output="", steps=[hit(BEAT, "assets/click.wav")])], bpm=bpm, from=FROM)
```

| `from` | `bpm` | click's beat | the edit | clicks that sound |
|---|---|---|---|---|
| 0 | 100 | `13/6` | `bpm = 110` at 1300 | 2, at 1299.9999999999998 and at 1300 |
| -4 | 60 | `7/5` | `bpm = 50` at 5400 | 0: the voice at 5400 ends `by edit` |
| -4 | 80 | `2/3` | `bpm = 70` at 3500 | 0 |
| 0 | 120 | `2` | `bpm = 150` at 1000 | 1 |

Expected: 1 in every row. LLP 1125 D3 says a new `bpm` moves tempo "without a jump in phase", and that an edit ends what has not started and expands the new value from `t`.

The commit has to fall on the hit's time to the last bit. A person's tap on a live clock does so only by chance. A timer's commit does (`after(1300, change)`), and so does an action run right after the clock is advanced to that time, as a test does.

## Verification

A temporary test in `runner/src/transport/tests.rs` ran each row twice, once with `Runner::advance` to the time and then `Runner::act`, once with a `task … after(…)`, and printed the voice table: the counts above, both ways. Its track also held a hat at beats 0 and 8, in no group. The JS target does the same (`host/web-js/transport.js:234`): rows 1, 2 and 4 with the track above, driven under Bun through `rt.js`'s `commit`, gave 2, 0 and 1. No output played.

## Direction

Decide both sides by one measure, so a hit is on one side of the edit whichever way its time rounds. For example, issue from the first beat the edit took back, and place a hit whose new time is before `t` at `t`; a followed clock's re-anchor already refills from the first beat taken back (`runner/src/transport/clock.rs:414`). Make the same change in `transport.js`.

## Related

LLP 1125's declared limits for edits cover a track's `offset` and an edit while following a clock, not `bpm`. `QUEUE.md`'s "LLP 1125 stage 3 (MIDI), what was not run" names two other edge cases. No issue covers this.
