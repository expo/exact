# An empty time input on the Mac opens at its `max` instead of the current time, and does not hold its `min`

**Status:** Closed
**Resolution:** Already fixed by c3eb81b7: `DateValue.initial` parses the current `HH:mm` on the day the bounds are parsed on and holds it within `min` and `max`, which `DateField.becomeFirstResponder` writes into an empty field.
**Systems:** host/apple/Sources/ExactKit/Mac/DatesMac.swift (DateField.becomeFirstResponder), host/apple/Sources/ExactKit/ControlKinds.swift (DateValue)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

An empty date control that takes the focus is given the wall clock's date and time, "Editing an empty one starts at now" (`host/apple/Sources/ExactKit/Mac/DatesMac.swift:75`, `:112-117`). For `type="time"` the picker's bounds are times on the day `DateFormatter` gives a pattern with no date, 2000-01-01: `configureDate` sets `minDate` and `maxDate` from `DateValue.parse("time", …)` (`DatesMac.swift:24-25`, `ControlKinds.swift:103`). The value written at focus is on today's date, after every such bound, and `NSDatePicker` keeps its value inside its bounds, comparing whole dates. So:

- With a `max`, the field starts at `max`, whatever the time is.
- With a `min` and no `max`, the field starts at the current time even when that is before `min`, and the picker goes on accepting times before `min`. The runner refuses each such `change`, so the field shows a time the app does not have. Once a value is bound the picker is on the bounds' day and holds `min`.

A `datetime-local` and a `date` keep real dates in their bounds and are not affected.

## The failure

```
input type="time" value=a change=setA min="09:00" max="17:00" placeholder="Office hours"
input type="time" value=b change=setB min="18:00" placeholder="Evening"
```

with `a` and `b` empty.

- At 16:17, focus the first. Observed: it holds 17:00. Expected: 16:17, the current time, which is inside the bounds.
- At 16:24, focus the second and press Up Arrow. Observed: it holds 16:24, then 17:24; `b` stays `""`; the field still shows 17:24 when left. Expected: it starts at 18:00 and goes no lower, as it does once it has a value.

## Verification

Run as a temporary Mac XCTest (`bun host/apple/build.mjs --test --only <class>`): the inputs above booted in a session in a window, each field made first responder, Up Arrow sent as the agent's `type … key ArrowUp` sends it.

```
office  (wall clock 16:17) focused: picker holds 2000-01-01T17:00; minDate 2000-01-01T09:00 maxDate 2000-01-01T17:00
evening (wall clock 16:24) focused: picker holds 2026-11-10T16:24; minDate 2000-01-01T18:00
evening Up Arrow: picker holds 2026-11-10T17:24; reported "17:24" input change; b ""
journal: change view 2 refused: InvalidValue { event: "change", reason: "\"17:24\" is before min \"18:00\"" }
```

Control: the same `min` on a field bound to `20:00` held 2000-01-01T20:00, and four Down Arrows reported `19:00` and `18:00` and stopped there. A `max="23:59"` field started at 23:59, and one with no bounds at 16:17.

## Direction

For `type="time"`, write the current hour and minute on the day the bounds are parsed on (parse `HH:mm` of now with `DateValue.parse("time", …)`), so the bounds compare times and not days.

## Related

`issues/20261110-date-field-now-follows-user-calendar.md` covers the calendar those same lines read the wall clock in. `issues/20261110-time-bounds-compared-as-strings.md` is the runner's comparison of a reported value with its bounds. An empty field reports only a change, so a time of exactly `max` can be entered only by stepping away and back: the rule `issues/20261110-mac-empty-date-cannot-take-the-date-it-shows.md` records for the Mac and `issues/20261110-empty-date-cannot-take-the-day-the-picker-holds.md` for iOS.
