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
