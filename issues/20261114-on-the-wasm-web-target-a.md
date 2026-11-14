# On the wasm web target a held MIDI note-on can sound after a freeze

**Status:** Open
**Systems:** host/web/wasm/commit.js, host/web/midi-glue.js
**Author:** Charlie Cheever
**Date:** 2026-11-14

<what is wrong, where, and how to reproduce it>

## What is wrong

On the wasm web target, a freeze that arrives while a tree update is held behind a presentation hold sends the replacement note-off at once, but a note-on already held for presentation stays queued: its scheduled time has passed and the runner does not retract it. When the tree is released the stale note-on is sent after the note-off, and nothing silences it. Example: a note-on at 100 ms and its note-off at 500 ms are held; the page freezes at 150 ms; the tree is released at 300 ms.

The JS target does not have this: it stops MIDI in the freeze commit before any deferred tree update. Found by review of the `frozen` page fact (issues/20261113-no-fact-for-a-suspending-host-or-a-phone.md).

## Fix

Associate held note-ons with their note-offs in host/web/wasm/commit.js; a freeze discards note-ons still awaiting presentation, and timed note-offs stay ordered after their starts. Regression: advance past the held note-on, freeze, release, and check the MIDI send order through midi-glue.

