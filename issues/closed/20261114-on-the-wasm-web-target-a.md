# On the wasm web target a held MIDI note-on can sound after a freeze

**Status:** Closed
**Resolution:** The wasm host marks the batch that commits the page's transition into frozen, and the presentation hold settles the output it still owns once on that batch: starts are discarded, and ends and cleanup are delivered in their original order.
**Systems:** host/web/src/batch.rs, host/web/src/host.rs, host/web/wasm/commit.js, host/web/midi-glue.js, host/web/sound-glue.js
**Severity:** P3
**Author:** Charlie Cheever
**Date:** 2026-11-14
**Related:** issues/closed/20261115-on-the-js-web-target-standalone.md

## What is wrong

On the wasm web target, a freeze that arrives while a tree update is held behind a presentation hold sends the replacement note-off at once, but a note-on already held for presentation stays queued: its scheduled time has passed and the runner does not retract it. When the tree is released the stale note-on is sent after the note-off, and nothing silences it. Example: a note-on at 100 ms and its note-off at 500 ms are held; the page freezes at 150 ms; the tree is released at 300 ms.

Found by review of the `frozen` page fact
(issues/closed/20261113-no-fact-for-a-suspending-host-or-a-phone.md). The JS
target's standalone presentation-held output is the related issue above.

## Resolution

The wasm host marks the batch that commits the page's transition into
`frozen`. The presentation hold consumes that mark on the batch's first
application and settles all output it still owns: operations that start
physical output (MIDI note-on, Start, Continue and Clock, a sound play, or a
haptic) are discarded; the remaining operations are applied in their original
order. Replaying the held freeze batch does not settle output created after a
resume, and the resume does not reconstruct discarded starts.

The first fix associated held note-ons with note-offs. That inference was
wrong: the runner's ordinary overlap retracts a moved note-off before emitting
the next note-on, and a live refill can hand a complete, already-due note pair.
In both cases the host deleted a valid note-on without a freeze. Non-freeze
batches therefore keep the behaviour of `ecdaf06fb`; only the explicit freeze
edge settles the hold.

The regression fixture is checked against batches emitted by the real web
host and runner, then passed through the real `commit.js`, document and
presence holds, and MIDI and sound glue. It covers the original held note, the
runner's overlapping notes, an already-due live refill, a short note whose
freeze batch contains no MIDI cleanup, clock Start and ticks followed by Stop,
resume before release, a direct haptic command, and sound and MIDI created
after that resume. These cases confirm that the defect was the class of
physical-output starts held past a freeze, not only MIDI notes.

Metronome on the wasm target stopped its sequence with reason `background` when
driven in Chrome with `prefer frozen true`. The agent substitutes MIDI ports and
does not call a physical `MIDIOutput.send`, so that drive cannot observe browser
send order; the recording-output regression observes it directly.
