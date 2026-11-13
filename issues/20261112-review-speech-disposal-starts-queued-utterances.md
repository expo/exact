# Disposing a speech batch starts queued utterances before canceling them

**Status:** Open
**Systems:** packages/speech-synthesis
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, disposing an active utterance and its queued
successor in that order starts the successor before canceling it. On the
web, `dispose` loops over the ids and calls `end` for each
(`web/index.js:209-218`). Ending the current utterance immediately calls
`proceed`, which submits the next queued utterance to the browser
(`:61-80`). That next id may be in the same disposal batch. It reaches
`speechSynthesis.speak` before the loop reaches it.

This breaks the package's stated guarantee that queued utterances whose
replies were dropped never reach the synthesizer. It can briefly start
speech that no caller owns, and reports the queued utterance as
`interrupted` instead of `canceled`. Apple and Android have the same
ordering: `dispose` calls `end`, which starts the next item before the
batch is retired (`apple/SpeechSynthesis.swift:475-483`, `:244-261`;
`android/SpeechSynthesis.kt:403-412`, `:194-210`).

A Bun 1.4.2 probe imported the actual web implementation, with a speech
engine stand-in recording calls. It created an active and a queued
utterance, then called `dispose([first.id, queued.id])`:

```text
before dispose ["first"]
after dispose ["first","queued reply will be dropped"]
queued outcome {"state":"ended","outcome":"error","error":"interrupted"}
```

The probe is `/tmp/exact-review-20261112/synthesis-dispose-probe.mjs`.
It verifies dispatch and queue state, not audible output or native speech
engines. Native paths were checked by source tracing.

Retire every queued id in the disposal batch before ending the current
utterance, then advance once to the first surviving item. Validate batches
in both orders, disposal while paused, and an unrelated queued utterance
that should still run. A dropped queued utterance must never be submitted
to the platform.

Checked all open and closed filesystem issues and all 275 cached GitHub
issue bodies. The language-model teardown report describes a similar
ordering error in a separate package; no existing report covers speech
batch disposal.
