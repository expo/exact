# A non-finite argument reaches a Rust source on the main thread, fails it on a worker, and is refused by a TypeScript source

**Status:** Closed
**Resolution:** Resource and send arguments contain only finite numbers, including upload contexts; unchanged NaNs and signed zeros preserve resource failures.
**Systems:** data/src/placed.rs (argument crossing), runner (resource arguments), js/src/lib.rs, LLP 1027.002 §1, LLP 1027.001
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Contract arithmetic is IEEE (`runner/src/vm.rs:624`): `1 / b` with `b = 0` is infinite, and the runner passes it to a source as a resource's argument. `docs/contract-for-agents.md:338` refuses a non-finite number in a state or derive ("every host can carry it"); an argument is neither. At `dd6f5dcd` three paths treat it three ways:

- A Rust source on the main thread receives it.
- The same source on a worker fails. `Placed` sends a worker's arguments as their canonical bytes (`data/src/placed.rs:147`) and reads them back with `Value::from_bytes` (`:151`), the plan decoder, which refuses a non-finite number. A worker's answer that holds one is refused too (`data/src/envelope.rs:127`).
- A TypeScript source refuses the call as `BadArguments("argument 0")` (`js/src/lib.rs:884`): LLP 1027 D2 marshals a `number` to the module as "a finite number".

Placement is meant to change when a call runs, with "the same inputs, results, grants and durable data" (LLP 1027.002 §1), and a move between the languages is meant to keep an operation's inputs and results (LLP 1027.001 §1).

No app in `apps/` places a Rust source on a worker, so the first split shows only in an app that does. The split between the languages is there on the main thread.

## The failure

```
component App
  state b = 0
  resource kind = kind(1 / b) as shape string
  view
    text kind testId="kind"
```

with a Rust source whose `kind` answers `"infinite"`, `"nan"` or `"finite"` for its argument, booted as `Placed::new(Kind, placement)`.

Observed, `Placement::Main`: `kind` is `infinite`. Observed, `Placement::Worker`: `kind` stays empty and the journal says

```
fulfil 1 (kind) [HTTP 200, 139 bytes] refused: Data { resource: "kind",
  error: Unavailable("an argument did not cross: NonFiniteValue") }
```

Expected: one outcome for both placements, and for both languages.

## Verification

Ran that Contract through a `Runner` for both placements (a test beside `js/tests/it`, `cargo test -p exact-js --test it`): `Some(Str("infinite"))` on main, `Some(Str(""))` and the journal line above on a worker. Asked the timers fixture's TypeScript module `spin(Infinity)` at `DataSource::answer`: `Err(BadArguments("argument 0"))`. At `Placed` alone (`cargo test -p exact-data --lib`), a worker-placed source that answers `f64::INFINITY` fails with `turn reply: NonFiniteValue` where the main-thread one answers it.

## Direction

Settle it before the source is asked: have the runner refuse a non-finite argument for every source, with a message that names the argument, since only an in-process Rust source can carry one. If the main-thread behaviour is the rule instead, `Placed` needs a crossing that carries what a `Value` can hold.

## Related

No issue or QUEUE entry covers the runner's arguments. `issues/closed/20260901-caltrain-data-malformed-arguments.md` added one data crate's own checks for finite arguments.
