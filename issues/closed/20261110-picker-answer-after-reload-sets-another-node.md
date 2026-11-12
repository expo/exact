# A date picker left up across a reload sets whichever control took its node's id

**Status:** Closed
**Resolution:** `Presenter::replaced` drops every date or time picker still up, journaling `picks_renewed`'s line, so its answer no longer sets the control that took its node's id in the new tree.
**Systems:** host/linux/src/presenter/requests.rs, host/linux/src/presenter.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A press on a date or time control where a shell takes requests (Android's reader) queues
`Request::Pick` and records `Waiting::Pick(node)` by view id
(`host/linux/src/presenter/requests.rs:772`). The answer is applied to that id if the node
there is a control a person can pick for (`answer_request`, line 646). A commit that
renews the view drops the waiting answer (`picks_renewed`, line 686: "its answer is
dropped").

A reload replaces the runner, and the new one numbers its views from the start again.
`Presenter::replaced` (`host/linux/src/presenter.rs:1386`) clears the state kept by view
id that it lists (choices, typed text, the open menu, hover) but not `requests.waiting`.
So a picker opened for a control of the old tree answers into the new tree: the control
that now has that id takes the value and its `input` and `change` run, though the person
chose it for another control.

On Android only the dev loop reloads, since the reader has no update store
(`host/android/README.md:1256`), so this is a development defect: an edit saved while a
picker is up.

## The failure

The old plan is `DATES` of `host/linux/src/presenter/requests_tests.rs`, whose first
control is the date `day` (`state day = "2026-10-16"`). The new plan puts another date
control before it:

```
component App
  state day = "2026-10-16"
  state due = "2026-01-01"
  action setDay(value)
    day = value
  action setDue(value)
    due = value
  view
    column
      input type="date" value=due change=setDue testId="due" height=30
      input type="date" value=day change=setDay testId="day" height=30
```

`serve_requests()`, tap `day`, take the request, `reload` with the new plan, then
`answer_request(id, Some("2026-11-02"))`. Expected: the answer is dropped, as for a
renewed view. Observed: `due` becomes `2026-11-02` and `day` stays `2026-10-16`.

## Verification

A temporary test in `host/linux/src/presenter/requests_tests.rs` ran those steps. It
printed `ids: day before 2, after: due 2 day 3` and
`answered true; due = "2026-11-02", day = "2026-10-16"`. Not run on an Android device.

## Direction

In `replaced()`, drop every `Waiting::Pick` with the journal line `picks_renewed` writes,
since no node of the new tree is the one the picker was opened for.

## Related

`QUEUE.md`, "A restart keeps per-node state that a new node with the same id then picks
up", records this class for the Apple presenter, whose `Presenter.reset` is a list kept by
hand as `replaced()` is here; its cases are Apple's and it does not name the painted
presenter. "Android module views: two gaps the module tap and event seam leaves", item 1,
is the module-view case and does not cover the date picker.
