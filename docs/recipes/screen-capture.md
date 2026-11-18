# Recipe: capture before showing the window

`@exact/screen-capture` supplies `captureScreens`, `screenRecordingStatus`
and `requestScreenRecording` (LLP 1178). Declare `device.screen-recording
shot.purpose` and `fs.write app:/tmp`; `shot.purpose` is a localized usage
string. Reading or drawing the files also needs `fs.read app:/tmp`.

```ts
import { captureScreens, screenRecordingStatus, requestScreenRecording } from '@exact/screen-capture';

// Ask from the foreground setup UI, before using the global shortcut.
if ((await screenRecordingStatus()).state === 'prompt') {
  await requestScreenRecording();
}

// In the shortcut handler, before showing the overlay:
const { captures } = await captureScreens({ directory: 'app:/tmp/freeze' });
```

Each entry is `{path, width, height, type, screen}`. The files are
`screen-0.png`, `screen-1.png`, …, sorted by the display's `left`, then
`top`. Width and height are physical pixels; `screen` has the display-list
fields and `clientRect`, its rectangle in the calling window's CSS pixels.
Draw the file there with an image, or on Windows and Linux a canvas.
Capturing into the same directory replaces the files and redraws each image
and canvas showing them, even one that asked before the file existed. On the
web (agent only), an image shows a capture only if its source is set after
it. A canvas on macOS or the web cannot draw an app file.
Set the hidden window's frame
before capturing; with no window, `clientRect` is null. The API neither
shows nor activates a window, and needs no `device.window-management` grant.

On macOS, request Screen Recording permission once from the foreground
setup UI. Capture checks permission without prompting. Windows needs no
OS permission dialog. Both omit the pointer. macOS requests sRGB; Windows
records GDI's SDR colors, which may look washed out on HDR displays.
Displays need not be captured at the same instant.

Optional `type` and `quality` follow `captureCanvas`; the answer's `type`
and filename identify what the host wrote. See [the reference](../reference.md#still-desktop-capture-exactscreen-capture)
for size limits, encoding and publication errors.

Under the agent, capture writes a colored card with each display's index
and physical size. A drive needs a scratch store (`--storage <name>`) for
the files. If the app imports `getScreenDetails`, its fixture supplies the
screens. To supply viewport rectangles too, use
`prefer extension screen-capture.screens=[…]`, including `clientRect` where
the fixture has a window; this overrides the display-list fixture.
The default is the same single display as window management's.
On macOS, `prefer permission.screen-recording granted` bypasses the
substitute's permission ticket. `--bind screen-capture` uses the OS and
requires a person present for a macOS drive.

Real capture is unavailable on the web, Linux, and mobile hosts. Web and
Linux implementations support fixture captures under the agent only.
`getDisplayMedia`, one display, a region, and a named window remain out.
