# Recording a stream

`startMediaRecorder` records a stream opened by `@exact/media-capture` to
an `app:/` file. It returns `MediaRecorderInfo` with the recorder's handle,
state, MIME type and full path. `stopMediaRecorder` finishes the file and
returns `RecordedMedia`. A stopped recorder keeps that result in
`mediaRecorderState.result`; repeated stops return the same result.

```contract
use startMediaRecorder, mediaRecorderState, MediaRecorderInfo from "@exact/recording"

mutation started as shape MediaRecorderInfo
resource recording = mediaRecorderState(recorder=recorderId) as shape MediaRecorderInfo else empty()
action record
  send started = startMediaRecorder(stream=streamId, path="app:/data/clip", maxDuration=5000)
```

Open the stream with `getUserMedia(video=true)` and use its video track's
ID as `exact-camera srcObject`. Pass the **stream ID** to the recorder.
The preview and a package's frame analysis share the camera session with
recording; opening another camera or leaving the app is unnecessary. The
app's overlay remains on screen; it is not burned into the recorded frames.

Declare `device.camera purpose.camera` to open the camera and
`fs.write app:/data` to write this example's file. Recording an admitted
camera-only stream requires no microphone grant. To record the camera with sound, also declare
`device.microphone purpose.microphone`, open `getUserMedia(video=true, audio=true)`,
and pass that stream’s ID to the same recorder. The web, iOS and macOS record
one file. Android returns `NotSupportedError` until an Android app asks for mixed recording.

On iOS and macOS one capture session supplies the camera frames and microphone PCM.
A mixed take starts once the MP4 writer has accepted samples from both inputs.
It stages the MP4 beside the destination and replaces the destination on successful finish.
The video uses the camera’s H.264 or HEVC encoder settings. Audio is AAC.
Ending either track finishes the take. iOS uses the microphone selected by the
app’s audio session. macOS selects the capture device by UID. The default
unprocessed track records. Mixed recording returns `NotSupportedError` during
a voice or video call or when `echoCancellation`, `noiseSuppression` or `autoGainControl` is true.

The capture callback masks the microphone once, before the writer or any other
reader receives the buffer. Disabling silences every unprocessed buffer,
dropping up to one buffer of sound captured just before mute. Enabling admits
only whole samples captured later than the session clock reading at enable plus
one buffer duration. If that clock is missing, stops advancing or changes, or
capture is interrupted or changes routes during mute or a pending enable,
the microphone stays silent until the clock advances again. The same guard
then starts at that later reading. Out-of-order buffers are silent.
A capture clock that reports plausible but false advancing time is outside this guarantee.

Start returns `NotReadableError` if the session supplies no usable pair of
tracks within five seconds, or if more than two seconds of audio precede
the first usable picture (message `the writer fell behind`). A tap installed directly on the audio engine
pauses while the camera session owns the microphone.

Pause omits captured samples. At start and resume the first picture and overlapping
PCM share the first overlapping PCM sample as their origin. A resumed segment follows the preceding samples on the file
timeline. A foreground-only take pauses with reason `background`. Other microphone
readers keep their leases. A media-services reset finishes the active mixed take
with reason `media-reset` and saves the readable portion. Under back-pressure the
writer counts dropped pictures in the recording log. Surviving pictures retain
their timestamps, so the previous picture stays on screen until the next. More
than two seconds of pending PCM ends the take with error `the writer fell behind`.
Stop allows the encoder two seconds to drain the queued sound, then saves the
playable prefix and reports that error. A reset retains its `media-reset` reason.
Capture reaches `maxDuration` at the first PCM buffer boundary at or after the
limit, so the file can exceed it by less than one buffer.

The Apple driver records up to 300 ms of 16×16 pictures with the microphone track’s shared 440 Hz signal.
Substitute and mock tests decode the MP4 and exercise the handoff between the
engine and camera session. No real camera or microphone was run for this change.
The [device checklist](test/device.md#mixed-microphone-mute-and-lip-sync-on-iphone) measures mute through an interruption and lip sync at both ends of a two-minute take.

`maxDuration`, when present, must be a positive finite number of milliseconds.
It stops recording automatically, excluding pauses, and publishes the finished
file in `mediaRecorderState.result` with reason `max-duration`. An app does
not need to poll or issue a stop at the bound. `pauseMediaRecorder` and
`resumeMediaRecorder` retain the recorder and its camera lease. Pausing a camera-only take
on Apple requires iOS 18 or macOS; an explicit pause on iOS 17 returns
`NotSupportedError`, and an interruption there finishes the recording.
On the web, the clock starts when the browser begins recording; reopening an
idle camera does not use up the limit. Ended tracks do not hold up a live
track's recording, and a take with no media answers `NotReadableError`.

For video, `RecordedMedia` includes `path`, `size` in bytes, `mimeType`,
`duration` in milliseconds, `width`, `height` and `frameRate`. Those last
three fields are absent for audio. Video metadata comes from the completed file,
with rotation applied to its dimensions; pauses and changed constraints cannot
leave the result describing the initial capture settings. Set the requested frame
rate on the camera track's `MediaTrackConstraints.frameRate`, not on the recorder.
The platform negotiates that ideal; the result reports the file's video rate.
Web computes the average from encoded samples. Android uses the file's frame
count and duration, or its stored rate when the count is unavailable. These hosts
omit `frameRate` when the file does not supply enough timing information.

Without `mimeType`, Android and Apple write MP4 using the platform encoder.
The web chooses MP4 where `MediaRecorder.isTypeSupported("video/mp4")` is
true, otherwise WebM. Omit the path's extension to have it added; an explicit
extension that disagrees with the chosen container is refused. Apple finishes
a camera-only capture as a movie and copies the video to MP4 without re-encoding.
The movie uses the camera's capture rotation at the start of recording.

Android attaches CameraX `VideoCapture<Recorder>` to the same lifecycle as
preview and `ImageAnalysis`. The camera decides which combination fits its
concurrent use-case limits. A rejected combination returns `NotSupportedError`
and restores the previous outputs; no analysis or preview is silently dropped.
Adding or removing analysis rebinds the shared session; its persistent CameraX
recording stays open across the rebind. Disabling and re-enabling the track
reattaches that recording; a refused reattachment stops it with an error.
Ending the track still stops the take.
An Apple session that cannot add its movie output reports the failure in the
recorder state. A file or encoder failure leaves no successful result.

On Android, a drive that sets `prefer extension camera.clip={"path":"assets/frames.mp4"}`
records the same frames as the viewfinder. The clip loops on the camera track's
clock. Pausing the recorder omits that interval from the file. The result's
duration, dimensions and average frame rate come from the encoded MP4.

Without a clip, Android writes the same three-frame fixture as the Apple and
web agents. The fixture is a 300 ms, 16×16 MP4 at 10 fps. All three agents time
the recorder and its duration limit on the session clock.

The API follows [MediaStream Recording](https://www.w3.org/TR/mediastream-recording/).
Native video uses [CameraX video capture](https://developer.android.com/media/camera/camerax/video-capture)
and [AVCaptureMovieFileOutput](https://developer.apple.com/documentation/avfoundation/avcapturemoviefileoutput),
within [AVCaptureSession's output limits](https://developer.apple.com/documentation/avfoundation/avcapturesession/canaddoutput(_:)).
