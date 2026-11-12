# A press on a date or time field with no picker runs the checkbox's toggle and logs `InvalidEvent`

**Status:** Closed
**Resolution:** A press on a date, time or datetime-local control ends after the picker test whether or not a picker opened, so where none does it has no default action and never reaches the checkbox's toggle.
**Systems:** host/linux/src/presenter/control.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`toggle_control_with_press` (`host/linux/src/presenter/control.rs:84`) opens a date,
time or datetime-local control's picker only where a shell takes requests or the agent
drives (`(self.has_shell() || self.agent) && ...`, lines 116 to 128). Only Android's
reader serves requests (`canvas.rs:1116`). On desktop Linux and Windows neither holds,
so the control falls past that test, is no select and no radio, and reaches the code
written for a checkbox (lines 142 to 171): it stores an own "checked" state for the view in
`self.controls` and dispatches `input` and `change` carrying a boolean. The runner
refuses a boolean for a date control, and the presenter prints the refusal.

Every click on a date or time field there writes one error line per handler to
stderr, marks the frame dirty and leaves a stale entry in `controls`. The app's state
does not change, so nothing is seen on screen.

## The failure

```
component App
  state day = "2026-10-19"
  state at = "10:00"
  action setDay(v: string)
    day = v
  action setAt(v: string)
    at = v
  view
    column
      input type="date" testId="day" value=day input=setDay change=setDay width=160 height=40
      input type="time" testId="at" value=at change=setAt width=160 height=40
```

With no shell and no agent, a mouse press and release on each prints:

```
exact: InvalidEvent { event: "input" }
exact: InvalidEvent { event: "change" }
exact: InvalidEvent { event: "change" }
```

and `controls` holds `Some(true)` for both views. Expected: no event and no state,
since the press has no default action here.

## Verification

A temporary test in `exact-linux`'s library booted this app with the CPU painter,
cleared the presenter's `agent` flag and pressed each field with `pointer_down` and
`pointer_up`. The lines above are its output, with `own state Some(true)` for each
field and `day`, `at` and the actions' record unchanged. Not run on a display.

## Direction

Return `true` for the three date kinds after the picker test, whether or not a picker
opened, so only a checkbox or a switch reaches the toggle.

## Related

LLP 1069.001's as-built for slice 4 records that Linux has "No calendar, and no
keyboard editing yet", and `QUEUE.md`, "The neutral look on Linux and Windows, what is
owed", item 6 records the missing picker icon. Neither covers the press taking the
checkbox's branch.
