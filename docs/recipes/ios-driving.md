# Recipe: driving iOS

**Compare with a reference**: `bun exact.mjs agent ios --touch platform … "clock settle"
"screenshot .exact/x.png compare ref/x.png"` includes the keyboard on a simulator it boots.
Differences are grouped by node (`x.side.png` pairs both images, `x.diff.png` adds the diff).
Set the status bar with `xcrun simctl status_bar <udid> override --time 9:41`.
Real taps reach menus, tabs, alerts and native module controls. Omit the flag for an offscreen menu row.

**Context menus**: with `--touch platform`, hold `tap <source> drag 0 0 press 1000`, then
`clock settle`. Select with `tap <action>`, or dismiss with `tap <root> at <x> <y>` on UIKit's
backdrop outside the menu and preview; `clock settle` after either. Use `layout` for offsets
from the root's box. `tap <source> contextmenu` delivers only the recognized event.

Check on the web (`verify`); end with `verify ios`: UIKit bars, light and dark, and a relaunch
([tests](tests.md#before-you-call-it-done)). Stale drives build first; `--prebuild` starts early.
With TypeScript data, edits to `.contract`, `app.ts` and its imports reach iOS as a baked plan
(log `.exact/ios-plan.log`). Manifest, asset, font, grant, `appId`, canvas and Rust data edits
rebuild the app.

`back` dismisses or pops. `type <root> <path>` opens a route from the `navigate=`
node’s testId. A compare starts with empty storage.

```sh
bun exact.mjs agent ios "clock data" "screenshot .exact/a.png compare ref/a.png" "tap add-item" \
  "clock settle" "screenshot .exact/b.png compare ref/b.png" back "type app /item/1" \
  "clock data" "clock settle" "screenshot .exact/c.png compare ref/c.png"
```

| Flag or op | What it does |
| --- | --- |
| `rotate` + `portrait`, `portraitUpsideDown`, `landscapeLeft` or `landscapeRight` | Wait for the simulator scene’s `screen.orientation` in `layout`. |
| `check`, `check dark`, `check both` | Clipped text, squeezed scrollers, covers, tap areas under 44×44 pt, contrast. |
| `--scale 3` | Screenshots at 3 px per point. |
| `screenshot <file> window` | Bars included; also the default with an alert, sheet or menu up. Film samples the agent clock; [UIKit motion](../../experiments/ios-film-cadence/README.md) needs a display recording. |
| `tap <id> down` `screenshot <png>` `tap up` | `<id>` shown pressed, then pressed once; `tap hold <ms>` moves the clock. |
| `clock settle` | Ends a transition, runs an `after(1, …)`; waits up to 1.5 s for UIKit's animations (4 s: a list gliding). |
| `--timing platform`, `--touch platform` | UIKit's own transition timing (default: frozen); real taps and drags. |
| `--sim <udid\|name>` (`EXACT_SIM`) | Simulator (default: the app’s). |

- Drive controls by `testId` (their `accessibilityIdentifier`); `tap` and `type` scroll their
  target into view.
- A wheel or `tap <scroller> drag 0 -300` moves content (and bars) that far; a real
  drag glides unless held 100 ms ([more](../contract-for-agents.md#drags-on-ios)).
- Tapping an alert or action-sheet button waits for dismissal. `clock data` reads the reply.
