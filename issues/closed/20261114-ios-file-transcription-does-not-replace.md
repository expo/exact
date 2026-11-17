# iOS file transcription does not replace a recorder take's partial transcript

**Status:** Closed
**Resolution:** Fixed by a70b03763: Apple file transcription replaces a partial transcript through a held directory, and module destruction waits for its writers.
**Systems:** packages/speech-recognition, host/apple
**Severity:** P3
**Author:** Claude (Fable 5.2), from a drive run on 2026-11-14
**Date:** 2026-11-14

## What was wrong

On `ecdaf06fb`, drive 6 left a second recorder take with a partial transcript
after the substituted foreground exit aborted live recognition. It then supplied
`the whole take` and pressed Transcribe. LLP 1133.012 §10 requires file
transcription of an incomplete take to replace the partial transcript and mark
the take complete.

The Apple speech module resolved the transcript directory through the package
context's file resolver. That resolver admits a missing path or a regular file,
so the existing partial-transcript directory was refused and the final answer
had no transcript path.

## Reproduction

`bun packages/speech-recognition/test/drives.mjs ios --only 6` on the lane's iOS
27.0 simulator passed `the window holds the last 20`, `finalCount reads 30`, `one
file of 30 lines` and `leaving the foreground aborts it`. `Transcribe replaces
the partial transcript` failed with:

```json
{"name":"take-2","label":"Take 2 · 3.0 s · level 35%","path":"app:/data/takes/take-2.wav","transcript":"first half","complete":false}
```

## What is fixed

The Apple package context now resolves a directory destination with the file
resolver's confinement and symbolic-link guarantees. An existing file is
refused, while an existing directory or missing destination is admitted. File
transcription uses that directory resolver, so it empties the partial result and
records the completed transcript.

Directory replacement now reopens every component without following links,
removes and recreates the final directory relative to the checked parent, and
keeps the new directory descriptor for every transcript write. Replacing an
admitted ancestor while cleanup waits on the writer queue therefore cannot
delete or write outside the take.

A recognition now captures its independent package context when it is made.
Queued preparation and failure logging use that context, not the recognition's
unowned module, so an ended recognition can finish its writer after the module
has been destroyed. Finish and abort release the transcript directory after
all earlier writes and before they answer their waiters. Opening a transcript
member is nonblocking before its file type is checked, so a FIFO or other
special file is refused without holding the serial writer.

Module destruction now waits for every recognition's writer, including an
ended recognition's queued final write, before it releases the native package
context that authorizes file replacement. The recognition holds no additional
native owner: the module's existing destruction boundary keeps that owner alive
only until its writers drain.

On the lane's iOS 27.0 simulator, the host drives for cases 1, 2, 3, 5 and 6
passed every check. Drive 6 now ends with `Transcribe replaces the partial
transcript` and a process-level check that no `.transcript` directory
descriptors remain after finished and repeated recognition. Case 4 is a tool
invocation with no door on an Apple host; `contract/cli/tests/it/module_seam.rs`
carries that proof. All ten `AppFilesLinkTests` pass, including the queued
ancestor-replacement race and a FIFO that has no reader.
