# The web resumes audio voices that a native suspend stopped

**Status:** Closed
**Resolution:** WebOutput::suspend stops every voice before suspending the context, so a resume starts only the voices the next sync asks for, as on Apple and Windows.
**Systems:** game
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`WebOutput::suspend` suspends the `AudioContext` and leaves the `BufferSource`s in `voices`. `resume` calls `unlock`, which resumes that context. The sources that were playing at hide start again when the context resumes. Apple stops the unit and queues a stop for every live id. Windows requests not-running and stops live ids. `SurfacePlayer::sync` does not run while suspended, and on the web the context's resume settles before the next frame, so the old graph is audible until sync bumps the epoch and replaces it.

## Where

`game/audio/src/web.rs`, `suspend`, `resume`, and `unlock`.

## How to reproduce

Play a voice, hide the page or interrupt the session, then show it again. The pre-hide sources are audible for a moment. The native hosts stay silent until sync starts new voices.

## Direction

Stop every web voice inside `suspend`, and resume only the context. The following sync still restarts from the world offset.
