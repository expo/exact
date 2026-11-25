# @exact/global-shortcuts

Read `globalShortcut(id="fade", keys="Control+Alt+F")` to register a chord
while the resource has a reader. Declare the `device.global-shortcuts` grant.
Bind `globalshortcut` on the root to start fading the window and
`globalshortcutup` to restore it. Each action takes the shortcut ID and an
optional `GlobalShortcutEvent` with `id` and `keys`.

| Host | Press | Release |
| --- | --- | --- |
| macOS | Carbon chord, modifier pair, Right ⌘ or Right ⌥ | Yes |
| Windows | Registered hotkey | Unsupported |
| Other hosts | Unsupported | Unsupported |

Read `globalShortcutStatus().releaseEvents` before offering a hold action.
Check the registration's `registered` and `reason` before displaying it as
active. Withdrawing a held registration sends its reader a release.

On macOS, `keys="MetaRight"` binds Right ⌘ and `keys="AltRight"` binds
Right ⌥. Each fires on press and release. The monitor sees only modifier
changes, so Right ⌘ fires as part of ⌘C and Right ⌥ fires when someone types
a character with it. Windows returns `reason="unsupported"` for both codes.
Hosts without global shortcuts report `NotSupportedError` for the operation.

Like modifier pairs, these triggers require the Accessibility grant. A missing
grant gives `reason="not-allowed"`. For a modifier trigger, `registered: true`
means the permission gate passed and the system accepted the monitor. It is
not proof of delivery. Modifier triggers depend on the system delivering
modifier-flag changes to a global monitor, as measured on macOS 27.2.

Under the agent, `tap @ref` simulates a complete press. On macOS,
`tap @ref down` and `tap @ref up` let a drive inspect a hold between its edges.

## Pointer buttons outside the app

Declare `device.global-pointer` and read the source while recording.

```contract
resource listener = globalPointer() when recording as shape GlobalPointer
```

Set `recording = true` from an in-window recording control or a registered
`globalshortcut` press. The shortcut works while the window is hidden if every
registrar of that registration acted as the app.
The runner checks the work that first makes the resource live.
`listener.listening` answers true when the monitor opens. The root
hears `globalpointerdown` and `globalpointerup`. Each takes a
`GlobalPointerEvent` with `screenX`, `screenY`, `button`, `altKey`,
`ctrlKey`, `metaKey`, `shiftKey` and `timeStamp`. The coordinates match
`getPointerPosition()`. Time is measured in milliseconds since system startup.
Button 0 is primary, 1 is middle and 2 is secondary.

While the recording window is hidden, clicks in Finder or TextEdit still
arrive. Inside the recorder, use the window's ordinary pointer events.
AppKit's [global monitor](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/MonitoringEvents/MonitoringEvents.html)
observes other applications and cannot intercept their input. In a macOS
27.2 (26B123) guest both trust checks were false. Left, middle and right
each produced a press and a release. No permission request or status call
applies to these buttons.

Set `recording = false` to stop. A closed gate retains its last answer,
so display `recording and listener.listening` for a recording indicator. Unmounting the component that owns the
last live read, replacing the runtime or ending the session also releases
the monitor. After a reload the listener reads false until the person starts it again.
Under a controlled drive, prefer an event record for
`global-shortcuts.globalpointerdown` or `global-shortcuts.globalpointerup`
while the resource is live. Windows answers `NotSupportedError`.
