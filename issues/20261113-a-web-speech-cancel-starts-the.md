# A web speech cancel starts the next utterance in the same turn

**Status:** Open
**Systems:** packages/speech-synthesis
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

Dropping the speaking utterance calls `speechSynthesis.cancel()` and then `end()`. `end` clears `current` and `proceed` calls `synth.speak()` on the next queued utterance in that same turn. `onerror` does the same.

Chrome drops an utterance spoken in the same turn as `cancel()`, and often never fires `end` or `error` for it. The queue has already marked that utterance speaking. It stays speaking, and every later line waits behind it.

## Where

`packages/speech-synthesis/web/index.js`, `dispose`, `end`, `proceed`, and `begin`.

## How to reproduce

Queue two utterances and drop the handle of the one that is speaking. The second is handed to `speechSynthesis` in the cancel turn. It never starts and never ends. A later `speak` stays queued.

## Direction

Do not call `speak` inside `cancel` or `onerror`. Start the next utterance on a later turn, and ignore the error from the utterance `end` already finished.
