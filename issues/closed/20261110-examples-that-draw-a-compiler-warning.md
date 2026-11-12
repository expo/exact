# Four examples a builder copies draw a compiler warning, which `verify` counts as a problem

**Status:** Closed
**Resolution:** Each example does what its warning asks (README's rows swipe to delete, the timer's readout is the list's first row, the poll's options keep ids, the seed goes after with), and the docs test now fails on a warning in an example a builder copies.
**Systems:** README.md, docs/recipes/timers.md, docs/recipes/editable-rows.md, docs/recipes/random-and-shuffle.md, contract/cli/tests/it/docs.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Line references are at `dd6f5dcd`.

`contract/cli/tests/it/docs.rs:1-3` says every example in the guides "compiles with no
diagnostics". The test collects errors only (`:233`), so a warning passes; the pattern test
below it does collect warnings (`:359-360`). Four examples teach what the compiler then
warns against, and `verify` counts each warning as a finding, so the example copied as
written cannot end `verify: clean`.

| Example | Line | Warning |
|---|---|---|
| `README.md`'s todo app | 407, a Delete `button` in each row | `lower-row-delete-button`: "write `delete=<its press>` on the row and iOS offers it as a swipe" |
| `docs/recipes/timers.md` | 43, the readout's `column` above the list | `lower-list-beside-fixed`: "on iOS the rows stop at it in a hard line … Put what it says in the list" |
| `docs/recipes/editable-rows.md` | 21, `for o, i in options key=i` | `lower-key-position`: "this key reads `i`, the row's position … what a row holds (its state, a field's text, its motion) follows the position" |
| `docs/recipes/random-and-shuffle.md` | 27, `deal(game, round, time.seed)` | `lower-resource-launch-identity`: "`q`'s kept answer is never shown at launch … Pass it after `with`" |

Two contradict their own documents. `timers.md:5-6` introduces its example as "its readout
and button outside the list the route scrolls, so they stay put", and
`docs/recipes/list-rows.md:67-69` says that arrangement "stops the rows in a hard line
(`lower-list-beside-fixed`)". `docs/start-here.md:33-34` says the rows of a list a person
deletes from swipe to delete; README's rows do not.

## The failure

`docs/recipes/timers.md`'s `contract` and `contract-test` blocks as `app.contract` and
`app.test.contract` in an app `exact new` made:

```
$ bun exact.mjs verify
compiler: 1 warning
  app.contract:35:11 warning [lower-list-beside-fixed] this box stays put above the list that scrolls this route, …
verify: 2 problems (1 finding, 1 at the largest text size; the tests passed)
```

`editable-rows`, `random-and-shuffle` and README's todo each end `verify: 1 problem (1
finding; the tests passed)`.

Expected: an example compiles with no warning, or the document says why the warning does
not apply to it.

## Verification

Ran `contract build app.contract --json` and then `verify` on each of the four, on the web
host; each build printed the one warning in the table. `symbols.md` also warns
(`lower-symbol-platform`); `QUEUE.md:9` already doubts that recipe, so it is left out.
`content-design.md` draws `tamber-unreachable` from `exact:content`, a note `verify` says
and does not count.

## Direction

Change each example to what its warning says (`delete=` on README's row, the readout in a
section's `header`, a key the record keeps, `with time.seed`), and have the docs test
collect an example's warnings as the pattern test does.

## Related

`QUEUE.md:592` ("The README's example after it says less") lists other debts of that
example, not this warning.
