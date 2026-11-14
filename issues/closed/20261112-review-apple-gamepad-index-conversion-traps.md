# Apple gamepad requests can trap while converting an out-of-range index

**Status:** Closed
**Resolution:** A request's index converts with Int(exactly:), so one that is not a whole slot answers NotFoundError, and the slot table takes only a requested slot in its range.
**Systems:** Gamepad package, Apple pad registry
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the public `playEffect`, `resetEffects`, and `setLight`
interfaces accept an optional number for `index`, without bounds
(`packages/gamepad/exact.json:88,110,122`). The Apple adapter converts
that number with `Int.init` before looking up a pad
(`packages/gamepad/apple/Gamepad.swift:60–61`). A finite whole number
such as `1e20` is valid at the interface but cannot fit in an Apple
platform `Int`. Swift's conversion traps instead of returning the
adapter's `NotFoundError`. The conversion happens even when no pad is
connected, as long as the host supplies its normal registry.

A related unchecked conversion exists for injected test pads at
`Gamepad.swift:328`: `min(Int(value), 15)` converts before clamping. A
negative injected index also reaches `ExactPads.take()` without a lower
bound check (`host/apple/modules/ExactPackage.swift:797–804`), where the
slot lookup subscripts the array with that negative index.

Verified the interface declaration and both callers against the registry
implementation. A small Swift standard-library probe confirms that the
public example is finite but unrepresentable:

```text
index=0.0 finite=true Int(exactly:)=Optional(0)
index=15.0 finite=true Int(exactly:)=Optional(15)
index=1e+20 finite=true Int(exactly:)=nil
```

The probe is `/tmp/exact-review-20261112/gamepad-index.swift`, run with
the existing Swift module cache. This verifies the numeric boundary;
the native module and an app were not launched, and the trapping
conversion was not executed in a host process.

Validate that an explicit index is a whole number in the supported slot
range before converting it. Return an ordinary request error for an
invalid index, and enforce the registry's own lower and upper bounds at
`take()` as well. Cover negative, fractional, excessively large, and
missing indices, including the injected-pad path and requests with no
connected controller.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. No report covers these gamepad index bounds.
