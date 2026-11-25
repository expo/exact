# @exact/screen-capture

Take still images on macOS and Windows with `device.screen-recording` and an
`fs.write` grant for the destination. Ask for Screen Recording once from the
foreground setup UI on macOS. The capture and lookup calls use that grant.

```ts
import { captureScreens, getWindowAtPoint } from '@exact/screen-capture';

const window = await getWindowAtPoint({ x: screenX, y: screenY });
const { captures: [image] } = await captureScreens({
  directory: 'app:/tmp/steps', window: window.handle, cursor: 'always',
});
```

`getFrontmostWindow()` returns the same window description. A window still
includes its desktop frame and its own pixels under overlapping windows.
Pass `screen` from `getScreenDetails()` to capture one display. Add `rect`
in CSS pixels from that display's top left to capture a region. Omitting
the selectors captures all displays. The default cursor setting is `never`.
The app's own visible windows participate in lookup and capture.

Window and selected display capture ran in a macOS 27.2 guest. Windows
keeps its all-display GDI stills. Native window lookups and any explicit
`screen`, `rect`, `window` or `cursor` setting return `NotSupportedError` on
Windows. The host cross-compiled for x86-64. No Windows executable was run
for this change.
See the [capture recipe](../../docs/recipes/screen-capture.md) for grant setup,
click mapping, limits and agent fixtures.

The display selector accepts the record that Window Management returns.
An out-of-bounds or empty region, a region without a display, or a window
combined with a display gives `TypeError`. A missing display or stale
window handle gives `NotFoundError`. The owning app has a name and pid,
plus `bundleId` on macOS. Use the frame from the capture to map a click into the image.

For an all-display freeze the displays can represent different instants.
macOS requests sRGB. The Windows default uses GDI's SDR colors, which
can look washed out on HDR displays. To show an app file on macOS or the
web use an image. Windows and Linux can also draw one on a canvas.
