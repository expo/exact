# DragIn sends Root (window) pixels instead of ExactView (page) pixels, shifting all drag-and-drop hit-testing by safe-area insets

**Status:** Closed
**Resolution:** Tracked the inset page view on DragIn and translated Root and FieldText drag coordinates into page-view pixels, with a Robolectric regression test.
**Systems:** Android Canvas host (DragIn / ExactActivity / painted presenter drop hit-testing)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Units, clocks and coordinates)

Verification: verified by reading.

At `9c0431fd5`, `DragIn` assumes `Root` and `ExactView` share an origin (`host/android/canvas/src/main/java/dev/exact/canvas/DragIn.kt:26-27`: *"The root and the page's view lie at the same place, so the root's pixels are the view's, which exact2 reads a touch in"*). That assumption breaks whenever the page frame has a non-zero left or top inset (`c[0] != 0` or `c[1] != 0`):

- `ExactActivity.edgeToEdge()` (`host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt:160,171,188,579-591`) lays `root` (`Root`) out edge-to-edge under the system bars and display cutout.
- Unless the root element sets `viewport-fit="cover"`, `Presenter::fit_window` (`host/painted/src/presenter/window.rs:151,183-194`) sets `frame = [safe[3], safe[0], (window.size.0 - safe[3] - safe[1]).max(1.0), (window.size.1 - safe[0] - safe[2]).max(1.0)]`, which `pack_chrome` (`host/painted/src/canvas/device.rs:202-209`) packs into `c[0] = px(frame[0])`, `c[1] = px(frame[1])`.
- `ExactActivity.chrome()` (`host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt:670-677`) places `view` (`ExactView`), `natives` (`Natives`), and `fields` (`Fields`) inside `root` with `leftMargin = c[0]; topMargin = c[1]`.
- `Root.onDragEvent` (`host/android/canvas/src/main/java/dev/exact/canvas/Access.kt:835,844`) forwards `Root`'s `event.x`/`event.y` directly into `DragIn.heard()` (`DragIn.kt:50,52`), which calls `Exact.dragOver(x, y)` (`DragIn.kt:112`) and `Exact.dropReleased(x, y)` (`DragIn.kt:131`). Likewise, `FieldText.onDragEvent` (`host/android/canvas/src/main/java/dev/exact/canvas/Fields.kt:54-60`) delegates to `DragIn.field()` (`DragIn.kt:75-86`), whose `inRoot(field, event)` (`DragIn.kt:102-107`) transforms field-local coordinates relative to `root::getLocationInWindow` instead of `view`'s origin, passing those `Root`-relative pixels to `Exact.dragOver`, `Exact.fieldDrop`, `Exact.dropReleased`, and `Exact.dragDropped`.

### Trigger
In any Android app without `viewport-fit="cover"` (the default) on a phone with a status bar or cutout, or on Android TV with 5% overscan, drag text, a link, or a file (from another app or via `DragOut`) over a `drop` zone or a hosted text field (`FieldText`).

### Actual vs. expected
- **Actual:** Every drag coordinate sent across JNI is shifted by `(+c[0], +c[1])` (e.g. 72–144 px downward for the status bar/cutout in portrait, and rightward for landscape cutouts or TV overscan). `CanvasHost` (`host/painted/src/canvas/drop.rs:12-41`) divides `(x, y)` by `self.scale` and passes them directly to `Presenter::drag_over`, `drop_released`, `field_drop`, and `drag_dropped` (`host/painted/src/presenter/file_drop.rs:126,144,170,265-270,382-385`), which hit-test in viewport/page points via `self.hit(point.0, point.1)` without subtracting `page_origin()`. Drag hover and drop hit-testing miss drop targets or hit nodes below the finger, `DropEvent` offsets are shifted, and `Presenter::field_drop` (`file_drop.rs:265-270`) returns `-1` when `drag_hit(point) != Some(id)`, causing `DragIn.field` (`DragIn.kt:85`) to steal text drops away from hosted `EditText` fields.
- **Expected:** `DragIn` sends coordinates in `ExactView`'s local pixels, matching `host/painted/src/canvas/drop.rs:1-41` (*"at the view's pixels, as a touch is told"*) and `ExactView.onTouchEvent` (`host/android/canvas/src/main/java/dev/exact/canvas/ExactView.kt:281-373`).

### Verification & sibling sweep
Verified by reading the exact Android Kotlin and painted Rust call chain above. Sibling coordinate conversions checked:
- `host/painted/src/presenter/access.rs:625,972-977` and `host/android/canvas/src/main/java/dev/exact/canvas/Access.kt:364,572-604` (`Access.hover` on `Root`): `access_tree` adds `origin: (frame[0], frame[1])`, so `Root` hover coordinates match.
- `host/painted/src/presenter/window.rs:396` (`in_gesture_edge`), `host/painted/src/presenter/bars.rs:963-972` (`bar_stops`), `host/android/canvas/src/main/java/dev/exact/canvas/DragOut.kt:88-89`, and `host/android/canvas/src/main/java/dev/exact/canvas/Dismissal.kt:35-36`: all explicitly convert between page/view and window/root frames.
- `host/windows/src/window/file_drop.rs:328-332`, `host/apple/Sources/ExactDragIn/DropIOS.swift:73,82,296`, and `host/apple/Sources/ExactKit/Mac/FileDragMac.swift:145`: convert through client/window view hierarchies without this mismatch.

### Suggested fix & regression test
Track the page view (or its `(originX, originY)` updated in `ExactActivity.chrome()`) on `DragIn`, subtract the page view's offset relative to `root` in `DragIn.heard()` (i.e. `event.x - originX`, `event.y - originY`), and compute `inPage(field, event)` relative to the page view (or subtract `(originX, originY)` from `inRoot`). Add a Robolectric test in `host/android/canvas/src/test/java/dev/exact/canvas/DragInTest.kt` (extending `twoFields`, lines 164-176) that lays out `fields` and a page view with non-zero `leftMargin`/`topMargin` (e.g., `(48, 96)`) inside `root` and verifies that `dragOver`, `fieldDrop`, `dropReleased`, and `dragDropped` all receive page-local coordinates.

### Siblings checked

Checked Android Access.hover/access.rs:625,972-977 (adds origin), window.rs:396 & bars.rs:963 (uses page_origin), DragOut.kt:88-89 & Dismissal.kt:35-36 (uses view::getLocationInWindow), ExactView.onTouchEvent (view-local), Windows file_drop.rs:328-332, iOS DropIOS.swift:73,82,296, and macOS FileDragMac.swift:145. None share this bug.
