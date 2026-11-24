# Recipe: the displays and the pointer

`@exact/window-management` (LLP 1178): the displays by the web's
[Window Management API](https://w3c.github.io/window-management/), and
where the pointer is, which the web cannot ask:

- `getScreenDetails()`: `screens`, each a `ScreenDetailed` (`left`, `top`,
  `width`, `height`, the work area `availLeft` … `availHeight`,
  `devicePixelRatio`, `isPrimary`, `isInternal`, `label`), and
  `currentScreen`, the one holding most of the app's window. Coordinates
  are CSS pixels whose origin is the primary display's top left, y
  downward, as `window.screenX` is. It is a live source: it answers again
  when a display is added, removed, rearranged or rescaled, and when the
  window moves to another display.
- `getPointerPosition()`: a send answering `MouseEvent`'s `screenX`,
  `screenY` (that space) and `clientX`, `clientY` (the viewport's CSS
  pixels, negative or past the viewport's size outside it), and `screen`,
  the display holding the pointer, with `clientRect`: that display's
  rectangle in the viewport.
- `windowManagementStatus()`: `{available, state}`.

The app declares `device.window-management` in its grants (`app.ts`).
Natively nothing asks the person. On the web the browser asks, and Chrome
asks only from a press: a read at load while the state is `prompt` fails
`NotAllowedError`, so offer a button that refreshes the resource.

To put a toolbar on the display that holds the pointer, in a window that
spans several, ask once and place it inside that display's `clientRect`:

```text
use getScreenDetails, getPointerPosition, windowManagementStatus, ScreenDetails, PointerPosition, WindowManagementStatus from "@exact/window-management"

component Overlay
  resource details = getScreenDetails() as shape ScreenDetails else empty()
  resource status = windowManagementStatus() as shape WindowManagementStatus else empty(available=false, state="denied")
  mutation at as shape PointerPosition
  action open
    send at = getPointerPosition()
  action allow
    refresh details
  view
    main
      if status.state == "prompt"
        button press=allow
          text "Show the displays"
      match at
        case some(p)
          match p.screen.clientRect
            case some(r)
              row position="absolute" left=(r.x + r.width / 2 - 160) top=(r.y + 24) width=320
                button press=open
                  text "Crop"
            case none
              text "No window"
        case none
          button press=open
            text "Start"
```

See [host support](../reference.md#the-displays-and-the-pointer-exactwindow-management) and [window bounds](window-bounds.md).

The agent supplies one 1440 × 900 display at scale 2, the pointer centred
and the window at its top left. A drive replaces
either with `prefer extension window-management.getScreenDetails={…}` or
`window-management.getPointerPosition={…}`, each a whole value; on the web
the first read raises a permission ticket. `--bind window-management`
reads the platform (macOS, Linux, Windows).
