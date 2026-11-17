# The JS difftest driver compares date and time bounds as strings

**Status:** Closed
**Resolution:** The JS drive parses date, time and datetime-local values and their bounds to milliseconds as the kernel does and applies control_payload's checks, a reversed time range and a space-split datetime included.
**Systems:** semantics
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`datetime` keeps a date, time, or datetime-local value when the text matches a regex and sorts between `min` and `max` as strings. The runner parses both sides to milliseconds. It rejects a non-date such as year 0 or 2026-02-31, accepts a datetime split on a space, and for a time whose min is after max refuses only a value strictly between them. String order disagrees: `12:00:00` sorts after `12:00`, `23:00` sorts after max `02:00`, and `10000-01-01` sorts before max `9999-12-31`. The driver then either sets `refused` and does not fire, or fires a value the runner rejects.

`difftest corpus --js` and `contract verify --js` report a divergence for a type both executors are specified to treat the same way. The comment in the driver cites `control.rs`, which now compares the parsed numbers.

## Where

`semantics/difftest/js/drive.mjs`, `datetime` and `deliverable`. `runner/src/runner/control.rs`, the date, time, and datetime-local arm.

## How to reproduce

Drive a `type=time` whose min is `12:00` and type `12:00:00`, and one whose min is `23:00` and max is `02:00` and type `23:00`. The JS drive and the runner disagree on whether the event fires.

## Direction

Decide delivery with the kernel's numeric parse and `control_payload`'s checks, including a reversed time range and a space in a datetime. Do not lexicographically compare the attribute text.
