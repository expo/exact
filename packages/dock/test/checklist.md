# The desktop modules by hand (LLP 1133.025)

`desktop.mjs` drives what the agent can on this Mac (`bun
packages/dock/test/desktop.mjs macos`), and the absent cells (`web`,
`linux`). This list is what needs a person: the system's own UI, a real
login, and Windows. Each step names what to see; anything else is a
finding. Run `desktop.mjs macos --only 1 --keep` to keep the fixture app it
built, then launch that app's bundle by hand (its path is printed).

## Mac

1. **Status item.** The fixture's star appears in the menu bar, tinted for
   the menu bar's appearance (light and dark). Click it: AppKit's menu with
   Open Desktop Fixture, New Note, a separator and a dimmed Unavailable.
   Open brings the window forward. The window has no star button in it.
2. **Drag it.** ⌘-drag the star to another place in the menu bar, quit and
   relaunch: it comes back where it was left (`autosaveName`).
3. **Close keeps running.** Close the window with the red button: the app
   keeps running (the star stays, no window). Open from the star shows it
   again. Hide Menu Bar Item, then close the window: the app quits.
4. **Dock menu.** Right-click the fixture's Dock icon: New Note above
   AppKit's own items; choosing it raises "composed" by one.
5. **Attention.** Request Attention, then switch to another app within a
   second (or use `requestAttention(critical=true)`): the Dock icon bounces
   once (informational), or until the app is activated (critical).
6. **Login item.** Open at Login, Hidden: System Settings › General › Login
   Items lists the fixture, and macOS shows its own notice. Log out and in:
   the fixture starts with no window and its star in the menu bar; Open
   shows it, and the login line reads "launched at login". Record whether
   `keyAELaunchedAsLogInItem` was seen (D6, Q9): "launched at login"
   present means it was. Then Don't Open at Login, and confirm Login Items
   no longer lists it. Never leave the fixture registered.
7. **Global shortcut.** With another app in front, press ⌃⌥⌘K: "Presses"
   goes up by one, the other app does not see the key. Use ⌃⌥⌘J moves it:
   ⌃⌥⌘K no longer reaches the app.
8. **Modifier pair.** In a build whose fixture reads `Shift+Shift`, with
   Accessibility not granted to the app: `not registered not-allowed`.
   Grant it in System Settings › Privacy & Security › Accessibility and
   relaunch: holding both Shift keys presses once.
9. **Menu-bar app.** `desktop.mjs macos --only 8 --keep`, then launch the
   kept `menubarfixture` bundle: no Dock icon, no menu bar, no window; the
   star's menu ends with "Quit menubarfixture", which quits it.

## Windows (waits for the NucBox)

Built for Windows so far: the host's services (`host/windows/src/desktop.rs`:
`RegisterHotKey`, `FlashWindowEx`, the `Run` value), the three packages'
Rust (`packages/*/windows/lib.rs`), `keepRunning`, the hidden login launch
and one instance (`host/windows/src/window/lifecycle.rs`,
`host/windows/src/instance.rs`), cross-checked on this Mac only. Not built:
the notification-area icon, a launch with no window at every launch, the
startup task and the modifier pair's hook (LLP 1133.025 §14). Run these in the NucBox's signed-in console
session (`docs/testing.md`, "Windows: the NucBox"), not over SSH, with a
development build of an app that depends on the three packages.

1. **Global shortcut.** With Notepad in front, press the app's chord
   (`Control+Alt+Meta+K` is Ctrl+Alt+Win+K): the app hears it once, and
   Notepad does not see the key. Hold it down: still once (`MOD_NOREPEAT`).
   A chord Windows keeps (Win+L) reads `not registered reserved`; one
   another app holds reads `in-use`.
2. **Attention.** Request Attention, then click Notepad within a second:
   the app's taskbar button flashes three times; with `critical=true` it
   flashes until the app comes forward. With the app in front the journal
   says "the window is in front" and nothing flashes.
3. **Login item.** Open at Login, Hidden: `reg query
   HKCU\Software\Microsoft\Windows\CurrentVersion\Run` lists the app with
   `"<exe>" --login hidden`, and Settings › Apps › Startup lists it, on.
   Turn it off there: the status reads `requires-approval`. Sign out and
   in with it on: the app starts, reads "launched at login" and shows no
   window; starting it from Start shows the window, and Task Manager lists
   one process. Then Don't Open at Login: the value is gone. Never leave it
   registered.
4. **Login Items Settings…** opens Settings › Apps › Startup.
5. **Keep running.** With `"keepRunning": true` in `host.windows.window`:
   the close button and Alt+F4 hide the window, Task Manager still lists the app and its global shortcut
   still answers. Starting it from Start shows the window as it was
   (`exactPage().restores` reads 1), and no second process stays in Task
   Manager. The app's own button that calls `close()` ends it.
6. **One instance.** With Shelf open (no `keepRunning`), its jump-list task
   Add Book opens Add Book in that window, and no second window appears.
