surfaces: android
agent: Claude Code (Claude Opus 5.5)
task: draw the app's existing home-screen widget on Android (LLP 1126.003, #631)

### Rough
- The widget's refresh failed with `filesystem: entity not found` until the
  app had run once and made its storage; the widget showed the preview props
  meanwhile, which looks like real data. Nothing to change in Shelf, but it
  cost a while to tell a missing file from a broken refresh.
- A clamped title's last line came out of the snapshot tree as the whole line
  plus "…": the ellipsis glyphs carry the line's original end as their range.
  The tree now takes the line's text from the kept glyphs only.
- Resizing drew the new layout over the old one: the launcher reapplies an
  update to the views it has, where `addView` appends. The content is emptied
  first now.
- The widget's settings screen was blank: under Android 15's edge-to-edge the
  content sat behind the action bar. It takes the window's insets now.
- `adb` held in a shell variable (`A="adb -s …"; $A …`) is not split in zsh;
  a function works.

### Lean in
- The widget is the same `ShelfWidget` component and `widgetLibrary` refresh
  as on iOS; nothing in app.json changed for Android.
- The kernel's lines placed one `TextView` each matched the device's own
  text measure to about a dp at font scales 1.0 to 2.0, across Latin, CJK,
  Arabic and emoji.

### Needed
- home-screen widgets on Android: provided by exact2 (LLP 1126.003)
- a tap that opens a tab: the widget's root is now `link href="/stats"`,
  which iOS takes as the widget's URL too

### Checkpoints
- In the launcher's picker as "Your Shelf", 2×2, with a preview drawn from
  the row; placed, it shows the book count and latest title in the system's
  colours, light and dark.
- Resized from 2 to 4 columns and back, the title re-wraps, then clamps to
  two lines with an ellipsis.
- A tap opens the app on Stats, cold or warm.
- After adding a book and leaving the app, the widget shows the new count
  and title.
