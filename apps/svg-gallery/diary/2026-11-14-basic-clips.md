surfaces: web, macos, ios, linux
agent: Codex, GPT-6 Astra
task: demonstrate CSS basic-shape clipping

## Rough

Percentage polygons and rounded insets failed at compile time. The diagnostic
still described the path parser's former absolute-command subset.

## Lean in

The gallery's existing pixel comparator covers CSS box clips alongside SVG clips.

## Needed

A clip that follows a box's size and cuts its children with it.

## Checkpoints

Compiler reproduction failed on the reported polygon before the parser change.

The compiler now accepts the shapes. Apple clipping tests pass, including a
polygon resized in place. Page 4 compares both polygon sizes, rounded inset,
circle and ellipse against the browser on macOS and the headless Linux host;
all eight fixtures pass on both hosts.

Unpainted AppKit containers rebuild their clips after layout. Layout-transition
clips follow the shown surface size. Native basic-shape clips on SVG children
remain deferred: builds name pixel paths and percentage-free SVG clip paths as
alternatives. Web SVG children keep CSS basic shapes.

Computed SVG clips keep `url()`, pixel `path()` and `none`; a native runtime
ignores a computed basic shape and logs why. Painted pointer hits keep the
resolved outline through ancestors and retained content. Web layout transitions
resolve basic shapes against the shown size for both content and background.

Filtered Apple transitions restore the final clip for masks and pointer hits.
The web intersects the authored shape with the shown padding edge, including
borders and rounded corners, and resolves keyword positions. Native SVG pixel
paths start at the fill-box origin. Regression checks cover these cases; the
web presence suite, kernel and painted tests, and macOS and iOS simulator
XCTests pass. The gallery was driven and captured on web, macOS and iOS.
