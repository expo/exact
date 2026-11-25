# Recipe: dock a window and lock its aspect ratio

On macOS and the web, `send bounds = getWindowBounds()` reads one
`WindowBounds` from `@exact/window-management`. Its `screenX` and `screenY` are the outer
frame's top left in the display list's coordinates. Its `outerWidth` and
`outerHeight` include the title bar.

This action docks 12 CSS pixels inside the Mac work area.

```text
action dock
  setBounds(DOMRect(x=details.currentScreen.availLeft + 12, y=details.currentScreen.availTop + 12, width=480, height=302), true)
  setAspectRatio(16 / 9)
```

Placement starts after the action. Omit the boolean for a single
step. Reduce Motion and the frozen test clock also use one step.
`setAspectRatio(0)` unlocks the content's aspect. Other hosts refuse the
two commands by name.

AppKit saves the frame after either the app or the person changes it.
Quitting while expanded can reopen the window expanded. Float's Swift
implementation keeps its compact frame across that quit.

See [floating window controls](floating-window.md) for drag regions, level and click-through.
