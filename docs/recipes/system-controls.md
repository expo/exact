# A button outside the app: controls

A control is a button the system draws outside the app: in Control Center,
on the Lock Screen or the Action button (iOS and iPadOS 18 and later), or as
a Quick Settings tile (Android 10 and later). A press opens one of the app's
locations or runs one of its `tool`s. Every rule and omission:
[Controls](../reference.md#controls-control-center-the-lock-screen-the-action-button-quick-settings).

```json
"controls": [
  { "kind": "record", "title": "Record an idea", "symbol": "mic.fill",
    "tool": "startRecording", "authentication": "always" },
  { "kind": "inbox", "title": "Inbox", "symbol": "tray", "url": "/inbox" }
]
```

A tool control's tool is an ordinary `tool` line with no required
parameter. Its press is the person's action (origin `action`), not the app
in front: it may write or start a Live Activity, but never asks for a
permission, opens the camera or starts speech recognition, and what it
leaves running (a countdown, a `then`) keeps those limits.

## Recording from a control

`apps/recorder` is the example (`startRecording` and what follows it): it
starts the declared `recording` Live Activity, then on its answer
`getUserMedia(audio=true)`, then `startMediaRecorder` into
`app:/data/takes/`, and `fails when not pressedTake` unless a take records.

- **Permission.** A press never prompts. With the microphone not allowed,
  `getUserMedia` fails `NotAllowedError` and the app asks from its own
  Record button. On Android the Live Activity is a notification, so with
  notifications off it fails too, and Recorder shows a row that asks
  (`requestNotificationPermission()`).
- **A failed start.** A failed `getUserMedia` dismisses the activity; a
  recorder that did not start stops the track and forgets the take.
- **Filing the take.** Stopping the recorder answers its file, which the app
  files, then ends the activity.

On Android the tile opens the app and the press records once it is in
front, since a tile grants no microphone. Another tile, leaving the app or
the screen turning off ends the press; a take it started records on. On a
Lock Screen with a PIN an `always` recording tile is refused; a `required`
one asks for the PIN, then opens the app. On iOS a locked-phone take awaits
a device run; until then only `EXACT_QUALIFY_CONTROLS=1` builds offer it.

## On Android

The tile's service is `ExactControl_<kind in UTF-8 hex>` (`record` is
`7265636f7264`):

```sh
bun exact.mjs android            # build and install
adb shell cmd statusbar add-tile com.exact.recorder/dev.exact.app.ExactControl_7265636f7264
adb shell cmd statusbar click-tile com.exact.recorder/dev.exact.app.ExactControl_7265636f7264
```
