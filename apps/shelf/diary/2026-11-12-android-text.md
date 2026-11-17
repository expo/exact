surfaces: Android
agent: GPT-6 Astra
task: bind the bundled Latin text recognizer for Add Quote from Photo

### Rough

A fresh Shelf APK build without a launcher icon failed writing `icons/res.key`
before creating its parent directory. Creating the local build directory let
it continue; the build script is unchanged. Shared disk space fell below
40 GB during verification, so builds paused until it recovered.

### Lean in

Shelf already imports text detection and checks its availability. The Android
package binding supplies the recognizer without changing Shelf's quote flow.

### Needed

Run the minified APK on a phone. The JVM tests replace ML Kit's recognizer and
cannot qualify recognition or registrar loading; an emulator run did (below).

### Checkpoints

- Twelve focused JVM tests pass: language matching and refusal for Serbian
  without `Latn` and a non-Latin script after an extlang, line results, all
  eight EXIF orientations with four distinct corners at the recognizer,
  failures, the agent path and recognizer reuse/disposal.
- Shelf’s minified release APK passes strict dependency verification. Its dex
  retains the public constructors of CommonComponentRegistrar,
  VisionCommonRegistrar and TextRegistrar, named in the APK’s manifest.
- On an Android 16 arm64 emulator, a small app with the package read the
  fixture image's four lines in the debug and the minified release build, read
  a copy with EXIF orientation 6 upright, read no lines from an image without
  text, and did all of it with the network off. No phone was used.
