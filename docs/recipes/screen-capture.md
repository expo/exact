# Capture a macOS guide step

Declare `device.screen-recording shot.purpose` and `fs.write app:/tmp`.
Localize the usage string. Drawing a file also needs `fs.read app:/tmp`.
On macOS, call `requestScreenRecording()` from foreground setup if
`screenRecordingStatus()` reports `prompt`. Captures use the existing grant.

```ts
import { captureScreens, getWindowAtPoint, getFrontmostWindow } from '@exact/screen-capture';
import { getPointerPosition, getScreenDetails } from '@exact/window-management';

const directory = 'app:/tmp/steps';
const { screenX: x, screenY: y } = await getPointerPosition();
const window = await getWindowAtPoint({ x, y });
const { captures: [step] } = await captureScreens({ directory, window: window.handle, cursor: 'always' });
const frame = step.frame!;
const imageX = (x - frame.x) * step.width / frame.width;
const imageY = (y - frame.y) * step.height / frame.height;

const front = await getFrontmostWindow();
const { currentScreen: screen } = await getScreenDetails();
await captureScreens({ directory, screen });
await captureScreens({ directory, screen, rect: { x: 10, y: 20, width: 160, height: 90 } });
```

Pointer and display-list reads need `device.window-management`. Window
lookups need only the recording grant. Each returns a session handle,
title, desktop frame and owning app. Capture includes the app's own
windows. A covered window yields its own pixels. The default cursor is
`never`. Window frames and pointer positions use desktop CSS pixels.
A display region starts at its top left. Subtract the display's `left`
and `top` from a pointer position to place a region there.

For a freeze overlay, set the hidden window's frame and call
`captureScreens({ directory })` before showing it. Each entry gives
`{path, width, height, type, screen}`, ordered by display left then top.
The image dimensions are physical pixels. Draw it at `screen.clientRect`
in window CSS pixels (null without a window). Each call writes fresh
files. Remove them after use.

An agent drive needs `--storage <name>`. It uses the display-list fixture
or `prefer extension screen-capture.screens=[…]` to include a
`clientRect`. Its window covers the primary display and its pointer sits
at the centre. On macOS, `prefer permission.screen-recording granted`
grants the fixture ticket. `--bind screen-capture` uses the OS and
requires a person present. Web and Linux support fixture captures only. Windows takes all-display stills.
Its native window lookups and explicit selectors or cursor settings return
`NotSupportedError`. No Windows executable was run for this change.

For encoding, image limits, errors and platform differences, see the
[package README](../../packages/screen-capture/README.md) and
[reference](../reference.md#still-desktop-capture-exactscreen-capture).
