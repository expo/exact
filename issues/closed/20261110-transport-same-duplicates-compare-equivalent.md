# `transport::same` is a second copy of `compare::equivalent`

**Status:** Closed
**Resolution:** `transport::same` is deleted, and the test for an edited sequence calls `compare::equivalent_all`.
**Systems:** runner/src/transport.rs, runner/src/compare.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`runner/src/compare.rs` is where the runner compares values, "once" (`runner/src/lib.rs:17`, at `dd6f5dcd`). Its `equivalent` (`compare.rs:132-144`) answers whether no expression can tell two values apart: numbers by their bits, a shared list, record or option at once, strings by their text, lists and records item by item.

`runner/src/transport.rs:232-248` defines `same`, documented as "numbers bit for bit (so `NaN` is itself), a shared list at once". It is the same relation, written again:

| pair | `compare::equivalent` | `transport::same` |
|---|---|---|
| two numbers | bits (`compare::same`) | bits |
| two lists or records | same allocation, or equal length and every item | the same |
| `some`, `some` | same allocation, or the inner values | the inner values |
| `none`, `none`; unit; bools | equal | `a == b` |
| two strings, inline or shared | same object, or equal text | `a == b`, equal text |
| different kinds | false | `a == b`, false |

`a == b` never reaches a number inside a list: two lists, two records and two options are matched before it. `same` of a value with itself is true, so the shortcut for one allocation changes no answer.

`same` has one caller, the test for an edited sequence (`transport.rs:804-805`), which is `!compare::equivalent_all(&self.raw, &s.args)` (`compare.rs:173`).

A change to what "the same value" means (a new `Value` variant, a different rule for a signed zero) has to be made in both, and only `compare`'s has tests of its own.

## Verification

Read both functions against `Value`'s eight variants (`plan/src/value.rs:34-51`) and its `PartialEq` (`value.rs:75-90`).

A temporary test compared the two over every ordered pair of 843 values: `0`, `-0`, three NaNs of different bits, both infinities, booleans, unit, `none`, inline and shared strings of equal and unequal text (the 14-byte boundary included), and options, lists and records of those nested three deep, each also as a second handle on one allocation. It printed `843 values, 710649 pairs, 1967 the same, 0 where the two functions differ`.

With `same` deleted and `compare::equivalent_all` at its call, the runner's other transport tests passed.

## Direction

Delete `transport::same` and call `compare::equivalent_all` at its one use.

## Related

`Play::same` (`transport.rs:365`) compares two read plays field by field and stays. No issue or QUEUE entry mentions either function.
