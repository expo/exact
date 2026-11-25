surfaces: android
agent: Codex (Astra)
task: Compare painted glyph positions and widths with Android's text controls.

### Rough

The emulator reports widths in pixels. At 12 sp and 420 dpi a nominal glyph-width query gave 180 px for ten Roboto digits. Both the default TextView and its run measurement gave 170 px.

### Lean in

A native view in the specimen can inspect its Android siblings. Reading the mounted Exact field exposed an Android 10 monospace face difference that a separately constructed reference field hid.

### Needed

Android 10’s Paint reports advances indexed by UTF-16 offset. A ligature can put its entire advance on its first offset. The public positioned-glyph API starts at Android 12.

### Checkpoints

The 16 sp punctuation probe on Android 17 measured 963.418 px in TextView and occupied one line at 368 dp. At 12 sp and 320 dpi on Android 10, tabular Roboto's 8 took one pixel more than the other digits in both Exact and TextView.

On Android 10 at 320 dpi, a font that expands one letter to eight identical glyphs measured 144 px. Reading only the line width hid a placement error at the eighth glyph. Comparing the glyph origins with the native drawing put it at 126 px in both.
