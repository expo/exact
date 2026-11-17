# Apple age-range requests trap on a large finite gate

**Status:** Closed
**Resolution:** An age gate above 150 now refuses TypeError on Apple and Android, so no gate reaches Apple's Int conversion out of range.
**Systems:** Age-range package, Apple argument validation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `requestAgeRange({ ageGates: [1e20] })` passes the
package's validation and reaches a trapping Swift integer conversion.
An invalid age gate can terminate the app instead of failing the request
with `TypeError`.

`packages/age-range/exact.json` declares `ageGates` as `number[]`.
The Apple implementation reads those values as doubles at
`packages/age-range/apple/AgeRange.swift:45`, checks them at line 46,
and converts each with `Int($0)` at line 47. Its `refused` function
at lines 63-67 requires nonnegative whole values and spacing between
gates, but sets no upper bound. `1e20` is finite and whole, so it passes
both the ordinary argument boundary and this guard. It exceeds the
64-bit `Int` range. The conversion occurs before the agent branch,
availability check, or platform request.

A bounded Swift probe ran the implementation's validation function
unchanged and checked whether its accepted input has an exact integer
representation:

```text
gate=13.0 refusal=nil intExactly=Optional(13)
gate=18.5 refusal=Optional("ageGates are one to three whole ages") intExactly=nil
gate=-1.0 refusal=Optional("ageGates are one to three whole ages") intExactly=Optional(-1)
gate=1e+20 refusal=nil intExactly=nil
```

The probe is `/tmp/exact-review-20261112/age-range-gate-probe.swift`.
It validates the guard and representability failure; it deliberately
does not execute the trapping conversion or open an age-range sheet.
The unchecked conversion in the request path establishes the crash.

Reject gates outside the supported age domain before conversion, or use
`Int(exactly:)` and return a typed error when conversion fails. Keep the
same bounds on every implementation. Validate a normal gate, a fractional
gate, a negative gate, and a finite whole value above the integer range.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The open Android age-range report concerns concurrent
request queuing. The closed argument-number reports concern nonfinite
inputs and decimal rounding; this input is finite and its failure is in
the package's unchecked integer conversion.
