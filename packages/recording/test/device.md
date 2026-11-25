# Recording on a phone, by a person (LLP 1133.009 §11, drive 9)

The drives a phone needs, with the recorder (`apps/recorder`) built as in
production: no agent, both packages bound.

- iPhone: `bun host/apple/build.mjs --ios --device recorder-apple --run`
- Android: `bun scripts/agent-android.mjs build recorder --apk`, then
  `adb install -r host/android/app/build/recorder/outputs/apk/release/app-release.apk`

Each step says what to do and what should happen.

1. **First take.** Tap Record. The system asks for the microphone; allow
   it. Speak for ten seconds at a normal voice: the bars rise to about
   half the height with your voice, fall back between words, and scroll
   left smoothly, a bar every 50 ms, without jumping; the time counts. Tap Stop: "Saved Take 1 · 10.0 s · level N%" shows, and the
   take's player plays your voice back at its length.
2. **Pause.** Record, tap Pause for a few seconds, Resume, Stop. The take
   is as long as the time recorded, without the pause.
3. **A call during a take.** Record, then call the phone from another
   one. The take pauses (Resume shows) while the call rings. End the
   call: nothing records until you tap Resume, and then the take goes on.
4. **Leaving the app.** The recorder writes `device.microphone-background`
   (LLP 1133.033 D3), so a take goes on while the app is hidden. Record,
   lock the phone for five minutes, unlock, come back and Stop: the take
   is five minutes long and plays back with what you said while locked.
   - iPhone: the orange indicator stays on while locked.
   - Android: while recording, the notification shade shows an ongoing
     "Recording" notification with a running timer; tapping it opens the
     recorder. `adb shell dumpsys activity services com.exact.recorder`
     lists `ForegroundService$Capture` with `types=0x00000080`
     (microphone) while the app is hidden. Swipe the notification away:
     the take goes on (Android 14), and Settings' Task Manager still lists
     the app.
   - Without the line (remove it from `apps/recorder/app.ts` and build
     again): leaving the app pauses the take on both phones, and Resume
     shows when you come back. Tap Resume, speak again, then Stop.
     Playback includes both spoken parts and excludes the time away; a
     moving timer alone is not a pass.
5. **A headset.** With Bluetooth earbuds connected, record a take: it
   records from the earbuds' microphone (iOS offers it by default).
6. **Revoking.** In Settings, turn the microphone off for Recorder and
   come back: "The microphone is off for Recorder…" shows with Open
   Settings, which opens the app's settings; Record is disabled.
7. **Sound during a take** (iOS). With a second take recording, play the
   first take's player: it plays from the speaker, not the earpiece, and
   the second take goes on (D7's `.playAndRecord` with
   `.defaultToSpeaker`).
8. **Media services reset** (iOS, Settings › Developer › Reset Media
   Services). During a take: the take pauses with reason `media-reset`,
   nothing records until Resume, and Resume records again.
9. **Two sessions** (macOS sample host, `bun scripts/smoke.mjs host --app
   recorder` on a Mac whose microphone answers): two sessions record from
   one microphone, each its own take, and one records on after the other
   ends.
10. **A take the Task Manager stopped** (Android 14 or later, LLP 1133.033
    D5). Record for twenty seconds with the app hidden, then stop the app
    as the Task Manager's Stop does: `adb shell cmd activity stop-app
    com.exact.recorder`. The notification goes. Open the recorder again:
    `adb logcat -s exact` shows `recording: recovered …/take-N.m4a`, the
    take is in the list after its next refresh (record and stop another
    take if it is not), and it plays back up to the stop.

## Mixed microphone mute and lip sync on iPhone

Use an app that records one camera and one microphone track into `take.mp4`.
Play a steady tone near the phone throughout. Start recording, face the camera,
and say “pop” near the beginning and end so lip closure and sound are easy to
compare. Mark times on the recording timer, which excludes pauses.

1. At 20 s disable the microphone track. At 35 s enable it.
2. At 55 s disable it again. At 60 s invoke Siri or take an incoming call.
   End the interruption, return to the app, and resume recording with the
   microphone still disabled. At 75 s of recorded time enable it.
3. Stop at two minutes. Copy the finished MP4 and run the following command.
   If your marks differ, adjust the start and stop seconds to sit inside each
   interval. The line for each interval prints its peak in dBFS.

```sh
for interval in '5 15' '22 33' '40 50' '56 59' '65 73' '80 90'; do
  set -- $interval
  printf '%s–%s seconds ' "$1" "$2"
  ffmpeg -hide_banner -nostats -i take.mp4 -map 0:a:0 \
    -af "atrim=start=$1:end=$2,astats=metadata=0:reset=0" \
    -f null - 2>&1 | awk '/Peak level dB:/ {peak=$NF} END {print peak " dBFS"}'
done
```

Run the loop in `sh` or Bash. The enabled intervals should contain the tone.
The three muted intervals, including the one after resume, should read silence
or the codec floor. Report all six levels, the actual marks, phone and OS,
and whether the sound leads or lags the lips at each end of the take.
