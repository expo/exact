# Recipe: a floating Mac window

Use `setWindowAlwaysOnTop(true)` to float above ordinary Mac windows on the
current Space. `setWindowIgnoreMouseEvents(true)` passes clicks to the window
underneath. A shortcut can turn click-through off again.
`setWindowOpacity(0.5, 250)` fades to half opacity over 250 milliseconds.
Omit the duration for an immediate change. Reduce Motion and the driver's
frozen clock also apply the opacity immediately.

A strip with `app-region="drag"` moves the window when dragged. Give a
button inside the strip `app-region="no-drag"` to keep it clickable.
A double click on drag ground takes the person's title-bar action. An
authored `dblclick` there takes precedence.

The three window commands are macOS commands. Other hosts journal the
refusal by name. The agent reports `floating`, `clickThrough` and `opacity`
in the window's state.

See [window bounds](window-bounds.md) for docking and aspect lock.
