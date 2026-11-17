# The sound arm keeps an entry for every voice ever played

**Status:** Closed
**Resolution:** The arm records a voice's start only while the device check runs, and each onset the check reports drops the voice it matched and every voice aimed before it.
**Systems:** host/apple/soundarm/SoundArm.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`SoundArm.push` records each Play's start time by voice id (`host/apple/soundarm/SoundArm.swift:178-182`):

```swift
guard exact_sound_room(mixer) > 0, exact_sound_push(mixer, kind, id, sound, at, gain, rate, pan) else { return false }
if kind == 1 { started[id] = at }
```

The only code that removes an entry is `report` (`:214-218`), which runs from the tap that the device check installs (`check`, LLP 1096 §5). ExactKit starts that check only under `EXACT_SOUND_CHECK=1` (`host/apple/Sources/ExactKit/SoundModule.swift:265`). In any other run nothing reads `started`, and it gains one entry for each voice. Voice ids never repeat: the runner numbers them with a counter (`runner/src/sound.rs:592-593`). The output lives as long as the session, apart from a media services reset or an idle stop for speech.

## The failure

Any app that plays sounds. Observed: the arm's memory grows about 65 bytes a voice, with no bound:

| app | voices an hour | growth an hour |
|---|---|---|
| a metronome at 120 beats a minute | 7,200 | 0.5 MB |
| a game at 10 sounds a second | 36,000 | 2.3 MB |
| a sequencer at 32 voices a second | 115,200 | 7.5 MB |

Expected: memory that does not depend on how many voices have played. The start times are needed only while a device check runs.

## Verification

Run on macOS: the arm compiled as `build.mjs` compiles it (the two C objects, then `swiftc -emit-library`), loaded with `dlopen`, opened with no sounds and its engine started, then driven through `exact_sound_op`. Each Play names a sound that is not loaded, so the mixer drops it and nothing is heard. The process's `phys_footprint` between runs:

```
opened: 6.0 MB
after 1000000 End ops: 5.9 MB
after 1000000 Play ops: 70.7 MB
after 1000000 more Play ops: 135.3 MB
```

Not run: iOS (the same source), or the device check, during which `report` filters the whole dictionary at every onset.

## Direction

Record a start only while `checking` is set, and drop entries older than the check can still match (it matches a voice aimed at most 0.1 s after an onset).

## Related

No issue or QUEUE entry names `SoundArm` or `started`. The mixer's own state is bounded (`EXACT_SOUND_SLOTS`, `EXACT_SOUND_RING` in `sound_render.h`).
