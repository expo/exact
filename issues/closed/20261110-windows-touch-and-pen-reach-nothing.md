# On Windows a finger or a pen on the screen reaches nothing in the app

**Status:** Closed
**Resolution:** The Windows host handles winit's `WindowEvent::Touch`, driving the presenter's contact with the first finger or pen down as a touch or pen pointer (`GetPointerType`), and reports the pointer as a mouse again at the next mouse event.
**Systems:** host/windows/src/window.rs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`. The Windows host's `window_event` (`host/windows/src/window.rs:1094`) handles the mouse (`CursorMoved`, `MouseInput`, `MouseWheel`) and ends with `_ => return` (`:1320`). It has no arm for `WindowEvent::Touch`, and nothing in `host/windows` or `host/ink` reads `WM_POINTER` or `WM_TOUCH`, the window's message hook (`:286`) included.

winit 0.30.13 turns touch and pen contact into `WindowEvent::Touch` and nothing else. It handles `WM_POINTERDOWN`, `WM_POINTERUPDATE` and `WM_POINTERUP` for every pointer type, sends `Touch` for each, and returns 0 without calling `DefWindowProc` (`src/platform_impl/windows/event_loop.rs:1985` to `:2129`). Windows makes mouse messages from a touch or a pen only in `DefWindowProc`: "if the application does not process this message, DefWindowProc may generate one or more WM_GESTURE messages … If a gesture is not recognized, DefWindowProc may generate mouse input" ([WM_POINTERDOWN](https://learn.microsoft.com/windows/win32/inputmsg/wm-pointerdown)). So no `CursorMoved` or `MouseInput` follows a touch.

**The failure.** On a Windows 11 laptop or tablet with a touch screen, tap a button in Shelf, drag its library with a finger, or tap with a pen:

- observed (by reading): nothing is pressed and nothing scrolls; the events are dropped at `window.rs:1320`;
- expected: a tap presses and a drag pans, as in every Windows app. LLP 1115.004 names touch as an input of this host: D3.8 ("a touch screen's input is coarse") and D8.1 ("The presenter takes `WM_POINTER` input").

**Verification.** Read only: the lines above in this repository, winit's source under `~/.cargo/registry`, and Microsoft's page. Not run: no Windows machine was available, and it needs a touch screen or a pen. The check to make there: tap a row of Shelf's library with a finger.

**Direction.** Add a `WindowEvent::Touch` arm that tells the presenter a touch pointer, as the Android reader does (`host/linux/src/canvas/input.rs:44`): `set_pointer_kind(PointerKind::Touch)` (`host/linux/src/presenter/pointer.rs:51`), then `pointer_down`, `pointer_move`, `pointer_up` and `pointer_cancel` for the first contact by its `id`, and `PointerKind::Mouse` again at the next mouse event. The system's inertia (D8.1's DirectManipulation, which LLP 1115.004 puts in S2) can come after; a tap and a drag should not wait for it.

**Related.** `QUEUE.md`, "Finish Windows menu, text-size and contrast qualification", lists "touch menu checks" as remaining, which assumes touch input arrives. `rules/DEFERRED.md` admits Windows for general apps and defers no input. No issue names this.
