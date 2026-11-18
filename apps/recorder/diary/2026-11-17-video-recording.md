surfaces: android
agent: Codex, GPT-6
task: Keep the recorder app's dependency locks current as its recording package gains video.

## Rough

Writing the Android locks builds the APK with a cold Gradle dependency cache.
Downloading and verifying the existing build tools took about thirty minutes;
the recording package's unit tests compiled and ran in seconds from the warm cache.
Progress naming the artifact being downloaded would make the wait easier to diagnose.

## Lean in

The camera track exposes its shared session. Recording can attach alongside a
viewfinder and frame analysis without an app bridge or another camera lease.

## Needed

- Video recording: provided by `startMediaRecorder` over a camera stream.

## Checkpoints

The app continues to use audio recording. Its Android dependency lock includes
the CameraX video module now required by `@exact/recording`. The release APK
build passes strict dependency verification.
