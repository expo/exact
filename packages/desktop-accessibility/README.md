# @exact/desktop-accessibility

A guide recorder can describe a click in another app using the control's accessible
name and role. Add `device.accessibility` to the app's grants. In a foreground
setup button, call `requestAccessibility()` if `accessibilityStatus()` reports
`prompt`. On macOS the request opens the system trust prompt asynchronously.
After changing trust in System Settings, return to the recorder to refresh its live
status. If it reports `denied`,
the person can enable the recorder in System Settings. Cross-app accessibility
requires an unsandboxed Mac build. Windows reports `granted` without prompting.

From work started by a press, a global shortcut or a device event, pass a point in
desktop CSS pixels. The recorder can be behind the app receiving the click.

```ts
import { elementAtPoint } from '@exact/desktop-accessibility';

const hit = await elementAtPoint({ x, y });
const control = hit.name ? `the ${hit.name} ${hit.role}` : 'the highlighted control';
const where = hit.app.name ? ` in ${hit.app.name}` : '';
const step = `Click ${control}${where}`;
```

The answer has `role`, `name`, a `frame` with `x, y, width, height`, and
`app` with `name` and `pid`. macOS supplies `bundleId` when available. If the system
identifies only the process, its pid is present and its app name is empty.
Windows uses the executable’s File Description for the app name, falling back
to its stem. If Windows withholds the image path, the pid still identifies
the app, its name is empty and `processName` is absent.
Unmapped roles are `generic`. An unnamed control and a secure text field
have an empty name. Static text exposed only as the hit’s value also has an
empty name. The system hit can be a desktop group, a sidebar row or
a label inside a control. The result describes exactly that element.

macOS reads the AX title, description, then the element's explicit title
label. AX positions and frames already use the primary display's top left.
A point on a display above or left of it can be negative. Each screen's
backing scale affects the captured pixels only. Windows maps UI Automation
pixels through the same per-monitor layout as `getPointerPosition()`.

An untrusted macOS lookup or a refused caller gets `NotAllowedError`.
A missing hit, owner or frame gives `NotFoundError`. A failed AX hit test
gives `OperationError`. A drive can script a hit with
`prefer extension desktop-accessibility.elementAtPoint={…}`. The fixture starts
granted. A launched macOS drive can also set `prefer permission.accessibility denied`
or `prompt` to exercise setup. Windows has
cross-compilation and coordinate-model coverage. No Windows executable has run.

On macOS an in-app recorder button can also call `getWindowAtPoint` and
pass its handle to `captureScreens`. Those calls need the screen-recording
grant and an app-origin call. A global shortcut or other device event can
label its point on its own.
To draw the highlight on the resulting image, use the capture entry’s desktop
frame and its pixel dimensions. With `hit` and the same point as above:

```ts
const window = await getWindowAtPoint({ x, y });
const { captures: [shot] } = await captureScreens({ directory, window: window.handle });
const frame = shot.frame!;
const sx = shot.width / frame.width, sy = shot.height / frame.height;
const highlight = {
  x: (hit.frame.x - frame.x) * sx, y: (hit.frame.y - frame.y) * sy,
  width: hit.frame.width * sx, height: hit.frame.height * sy,
};
```

The [capture recipe](../../docs/recipes/screen-capture.md) gives the capture
imports, output directory and grants.
