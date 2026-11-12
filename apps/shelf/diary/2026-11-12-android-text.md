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

Run the minified APK on an Android emulator and a phone. The JVM tests replace
ML Kit's recognizer and cannot qualify recognition or registrar loading.

### Checkpoints

- Twelve focused JVM tests pass: language matching and refusal for Serbian
  without `Latn` and a non-Latin script after an extlang, line results, all
  eight EXIF orientations with four distinct corners at the recognizer,
  failures, the agent path and recognizer reuse/disposal.
- Shelf’s minified release APK passes strict dependency verification. Its dex
  retains the public constructors of CommonComponentRegistrar,
  VisionCommonRegistrar and TextRegistrar, named in the APK’s manifest.
- No emulator or phone run was made.
