surfaces: web, ios, macos
agent: Codex
task: Present calendar pickers and editing with platform semantics.

### Rough
The month already pads its foreground with document safe-area insets. Making it a route added the host's clearance too; the route now opts out while the editor keeps sheet clearance. A kind selector inside the editor header became three native bar buttons, losing its selected segment. It now sits in the sheet body. Delete confirmation keeps a separate Cancel control so sheet dismissal closes editing rather than choosing Keep.

The iOS driver reports the calendar input surface over a date's tap point. A wide viewport on the phone also leaves the wide editor's Cancel outside the physical viewport. These stopped the native confirmation-dismissal probes; browser and macOS drives cover the dismissal handler, while an actual sheet swipe remains unqualified.

### Lean in
Popovers and a modal route provide presentation, background exclusion and dismissal from shared Contract structure.

### Needed
- anchored pickers: provided by auto popovers
- form presentation: provided by a modal route
- kind choices: provided by a radiogroup in the sheet body

### Checkpoints
- Chromium: both widths pass popover dismissal, modal accessibility and focus, and Escape and Cancel during delete confirmation.
- The baked Contract tests check one safe-area inset for artwork and controls and keep kind choices out of the native bar.
- macOS with platform chrome: the theme popover, kind selection, Cancel and Escape during delete confirmation pass.
- iOS with platform chrome: the header begins at the 62-point top inset, the add button clears one 34-point bottom inset, wallpaper reaches the top, and the sheet body's chosen kind retains its selected styling and accessibility state.
