# Android startup recovery can consume a recording that is still active

**Status:** Open
**Systems:** packages/recording/android
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, `RecordingModule` starts recovery on an
unawaited thread when background microphone recording is enabled
(`Recording.kt:64-68`). The first `startMediaRecorder` can proceed while
that thread is pending (`:78`, `:98-119`). There is no barrier between
startup recovery and new `PartWriter` instances.

Every writer immediately registers its path in the same persistent ledger
that recovery scans (`Parts.kt:21-36`). Recovery treats every entry as a
take left by a dead process (`:118-129`); successful conversion deletes its
part file (`:112-114`). If recovery reads the ledger after a new take has
written its first frame, it converts and deletes that live take's part.
Later writes continue through the open descriptor to the unlinked file.
Stopping the take then throws `FileNotFoundException` when `finish` tries
to read the part by path (`:65-67`, `:74`). The recovered file contains only
the prefix written before recovery. `Recorder.finish` does not catch this
exception before completing the stop or releasing the track
(`Recording.kt:300-324`).

A controlled probe compiled the actual `Parts.kt` with small stand-ins
for Android preferences, media format, and muxer. It opened a writer,
wrote one frame, ran recovery while the writer remained open, wrote a
second frame, and stopped the original writer. The real filesystem and
the production part and ledger code produced:

```text
before recovery: active part exists=true, bytes=9
after recovery: active part exists=false, recovered bytes=2
original stop: FileNotFoundException
retained payload=[1, 2]
```

The second frame's payload was `[3, 4]`. The probe is
`/tmp/exact-review-20261112/recording-active-recovery/run.py`. It controls
the recovery ordering and uses a muxer stand-in; it does not exercise an
Android device, AAC encoding, or the frequency of the startup race.

Finish recovery of entries from the previous process before admitting
new takes, or separate those entries from active writers with an ownership
record and synchronization. Also complete stop with an error and release
its resources when finalization fails. Validate a delayed startup recovery
with a new recording, ordinary recovery after process death, and a failed
conversion while a writer is active.

Checked all open and closed filesystem issues and all 275 cached GitHub
issue bodies. The web recording write-failure report concerns successful
answers for failed writes. The recording path races noted in `QUEUE.md`
concern links in filesystem paths; neither covers recovery deleting a live
recording.
