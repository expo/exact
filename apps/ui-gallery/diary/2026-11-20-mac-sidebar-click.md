surfaces: macOS 27.0.1 and 27.2.1
agent: Codex
task: Open a gallery section with a background click and keep its first heading visible.

### Rough

On macOS 27.2.1 the driver's click reached the Typography row, but the table kept its selection empty. A plain AppKit table behaved the same way in an inactive application. It selected the row when the receiving window reported that it was key during the click. The same background click already worked on 27.0.1.

### Lean in

The Mac window's top inset is available as `env(safe-area-inset-top)`. The sections route can keep its heading below the controls on first launch and when a detail supplies the window title.

### Needed

- Background pointer input for native Mac controls, fixed in the driver.
- Title-bar clearance, provided by the Mac safe-area inset.

### Checkpoints

- On both Macs, clicks opened Typography and then Text colors. AppKit reported the first and second selectable rows on 27.2.1.
- Window screenshots on 27.2.1 show the Text heading below the title bar before and after selection. Keyboard navigation also opened the next detail.
