# A session destroyed while a hatch frame ticket is live keeps its display link

**Status:** Closed
**Resolution:** `ExactSession.destroy` drops the hatch clock's tickets before `frames.run(false)`, so the display link stops whatever frame tickets were live.
**Systems:** host/apple/Sources/ExactKit/Session.swift (`destroy`), Frames.swift, HatchClock.swift, NativeModule.swift (`destroyModule`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`ExactSession.destroy()` says "Everything attributable to this session goes (D2): the display link and clock, …" and stops the link with `frames.run(false)` (`Session.swift:1480`, commit `dd6f5dcd`). `Frames.run` keeps the link while a module's `context.frames` ticket is live, whatever it is asked (`Frames.swift:165`):

```swift
let on = (wanted || hatches) && session?.clock == nil
```

The ticket is dropped only later in `destroy()`: `natives.destroyModule()` (line 1497) calls `hatchClock.reset()`, which sets `frames.hatches = false` and does not call `frames.run` (`HatchClock.swift:65`). So `destroy()` returns with the link still on the main run loop (on iOS, with the session still asking the app's `FrameClock` for frames).

What follows depends on who still holds the session:

- Released, with its view and window: the `Frames` object and the link are released with it. Nothing is left.
- Still held, its view still on a display: the link calls `Frames.tick` on the destroyed session. The callback's last line, `run(motion || canvas2d || timerSoon || …)` (`Frames.swift:127`), stops the link only if none of those flags was set when the session was destroyed, and `destroy()` clears none of them. A session destroyed while motion ran keeps its link for as long as it is held, each frame running `timerAdmissions.check`, the canvases' frame and a `runtime.tick` that the destroyed runtime refuses.

ExactMac closes the window and the sample hosts remove the view, so in this repository the link goes with the session; an embedder that keeps a destroyed session's view on screen keeps the callbacks.

## The failure

```swift
// a booted session in a window
session.natives.hatchClock.natives = session.natives   // as NativeViews.module wires it
session.natives.hatchClock.frames(1, on: true)
let link = try XCTUnwrap(session.frames.link)
session.destroy()
XCTAssertEqual(session.natives.hatchClock.liveTickets, 0)   // passes
XCTAssertNil(session.frames.link)                           // fails
```

Observed: `XCTAssertNil failed: "<_NSDisplayLink: 0x7c1a411000>" - destroy left the display link running`. Calling the link's callback afterwards (`session.frames.tick(link)`), the link stopped after one callback; with `session.frames.motion = true` set before `destroy()`, as `apply` sets it from `batch.motion`, it was still running after five. The control, a link started by `motion` with no ticket, is stopped by `destroy()`.

## Verification

Run as temporary Mac XCTests (`bun host/apple/build.mjs --test --only <Class>`). The ticket was made on the clock directly, not by a loaded module, and the callback was called by hand: the test machine presented no frames to the tests' windows. A second test released the destroyed session with its window and found the session, its `Frames` and the link all released a second later. The UIKit path is read, not run.

## Direction

In `destroy()`, drop the tickets before stopping the link (`natives.hatchClock.reset()` ahead of `frames.run(false)`, or clear `frames.hatches` there), so `run(false)` invalidates it.

## Related

No issue or QUEUE entry covers it. QUEUE's "Android has no session end for `destroy`" is another host.
