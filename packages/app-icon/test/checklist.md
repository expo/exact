# @exact/app-icon: drives and qualification

The app is Shelf (`apps/shelf`): its Stats tab's App Icon section offers Shelf's
own icon and Paper (`app.json`'s `alternate_icons`). The runner's substitute is
covered by `contract/cli/tests/it/app_icon.rs`; these drives use the real
implementation.

## iOS simulator

2026-11-17, Xcode 27.2 (27B52), iOS 27.2 (24B8117), iPhone 18 Pro.

```
bun host/apple/build.mjs --ios shelf-apple --sim "iPhone 18 Pro"
EXACT_UI_TEST_SOURCE=packages/app-icon/test/ShelfAppIconUITests.swift \
  bun packages/quick-actions/test/apple-ui.mjs <udid> <out dir>
```

`test1ChooseKeepAndRestore` passed in 34.9 s:

| Step | Result |
| --- | --- |
| Paper chosen | iOS's alert "You have changed the icon for “Shelf”." over the app, drawn with Paper; after OK, Paper is checked and the Home Screen shows it |
| Relaunch | Paper is still checked: `alternateIconName` is iOS's, kept across launches |
| Paper chosen again | no alert: the icon already shown asks iOS nothing |
| Shelf's own chosen | the alert again, with the primary; the Home Screen shows it |

`setAlternateIconName`'s completion handler ran only once the alert was
dismissed, so `setAppIcon` answers after the person's OK.

The same steps under the agent with `--bind app-icon` show the alert too, but
the drive cannot press its OK, and `clock data` gives up on the request in
flight; the XCUITest drive is the one that presses it.

## Still to run

- An iPhone and an iPad (§11 item 5), and an iPad's own alert.
- An update that removes the chosen alternate: what iOS shows, and
  `setAppIcon()` restoring the primary.
- A Linux-built bundle (`host/apple/car.mjs`) installed and switched.
