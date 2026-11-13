# A Material menu taller than the window cannot scroll

**Status:** Closed
**Resolution:** Material's menu is held to the window less the margins it keeps and scrolls past that, under a finger that moves past the touch slop and under the arrows, with square rows and no scrollbar as Compose's DropdownMenu has.
**Systems:** host/linux, host/android
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A top-layer menu on Android is measured with `Layout::of`. That path sets the menu's height to its full content and installs no scroller. The reader shows that size as a `PopupWindow`. Rows past the screen cannot be touched. Arrow keys move the focused row and `reveal_menu_target` returns without moving it, because there is no clip.

The desktop measure clamps the menu to the window and attaches `menu::Scroll`, so a wheel and the arrows bring a row into view. Android boots `serve_windows` and does not enable that overlay path. A `select`'s own menu is a different surface and already scrolls.

## Where

`host/linux/src/presenter/top/look.rs`, the height and `menu` of `Layout::of`. `host/linux/src/presenter/top/look/menu.rs`, `reveal_menu_target`. `host/linux/src/presenter/top.rs`, the measure chosen when the overlay is off, and `place_surface`. `host/android/canvas/src/main/java/dev/exact/canvas/Surfaces.kt`, the menu `PopupWindow` height.

## How to reproduce

On Android, open a menu popover with more rows than the screen. The window is the full list. Rows below the screen take no touch. ArrowDown moves the focus and the row stays off screen.

## Direction

Clamp the Material menu to the window and attach `menu::Scroll`, as the desktop measure does, so a touch and the arrows move the rows.
