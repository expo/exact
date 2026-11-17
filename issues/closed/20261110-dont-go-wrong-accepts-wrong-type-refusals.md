# `dont_go_wrong` counts the runtime's own wrong-type refusals as legitimate

**Status:** Closed
**Resolution:** The checks at a derive, a slot written and a slot initialized report a value that is not even `typed` as `Err.type` (`unfit`), so `dont_go_wrong` now proves, from the body's and initializer's typing, that for a checked program they refuse only a number that is not finite; the docstring's `unsupported` list names what the semantics leaves out today.
**Systems:** semantics/Contract/StepSound.lean, semantics/Contract/BootSound.lean, semantics/README.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `dd6f5dcd`.

The theorem (`semantics/Contract/BootSound.lean:391`) and what its conclusion means (`semantics/Contract/StepSound.lean:45`, `:67`):

```lean
theorem dont_go_wrong {p : Program} (hcheck : check p = true) :
    (∀ o, OutcomeOK (boot p o).2) ∧
    ∀ c, Reachable p c → ConfigOK p c ∧ ∀ o (ev : Event),
      ConfigOK p (ev.step p o c).1 ∧ OutcomeOK (ev.step p o c).2

inductive Legitimate : Err → Prop
  | refused (why : String) : Legitimate (.refused why)
  | unsupported (what : String) : Legitimate (.unsupported what)
```

`OutcomeOK` is `Legitimate` of a refusal's or poison's error. So the theorem excludes `Err.type`, `Err.unbound` and `Err.pending`, and admits every `Err.refused`, whatever its text.

The README, under "What is proven" (`semantics/README.md:902`): "commits, or refuses or poisons for a `Legitimate` reason (`refused`: the data seam, a non-finite number at a boundary, the host's input, the router, a row slot outside its row, a settlement cycle, fuel or the timer fire limit, a full queue, a task's key that is no key, an earlier poison; `unsupported`)", and (`:920`) "commit or fail for a `Legitimate` reason — never a type error, an unbound name or a `pending` read". The docstring above `Legitimate` gives the same list.

The runtime reports its own type checks as `Err.refused`: "arity", "an argument of the wrong type", "a write of the wrong type" (`semantics/Contract/Runtime.lean:905`, `:907`, `:929`), a slot "initialized with the wrong type" (`:1025`), a derive "of the wrong type" (`:334`). The write, initializer and derive checks are each one `conforms` test (`Runtime.lean:95`), which fails for a wrong type and for a non-finite number alike. `runAction_sound` discharges them with `refusedS : ∀ w, Strict (.refused w)` (`StepSound.lean:257`), without the body's typing. So nothing proved says that, for a checked program, these checks fail only on a non-finite number, which is what the README's list says of them. The parts exist: `exec_sound_ty` (a body's writes have the slots' `ValTy`, `Soundness.lean:1486`) and `conforms_of_valTy` (`ValTy` and finite numbers give `conforms`, `StepSound.lean:486`), used so far only for curried arguments (`curried_conform`, `:520`).

The docstring (`StepSound.lean:43`) also still lists frame tasks and a mutation's `then` as `unsupported`; both are modelled (`Runtime.lean:568`, `:700`).

## The failure

```lean
def bad : Program :=   -- check bad = false: a string written to a number slot
  { states := [{ name := "n", ty := .number, init := .num 0 }],
    actions := [{ name := "a", params := [], body := [.assign "n" (.str "x")] }],
    view := [.element "button" [] [("testId", .str "b")] [("press", "a", [])] []] }
#eval Observe.run bad {} [.tap "b"]     -- … "outcome refused\n# refused: a write of the wrong type" …
example : OutcomeOK (.refused (.refused "a write of the wrong type")) := .refused _
example (w : String) : OutcomeOK (.poisoned (.refused w)) := .refused _
```

The same program writing `n / 0` passes `check` and is refused with the same text.

## Verification

`lake env lean` on a file holding the above, in `semantics/`: it elaborates, prints `false` then `true` for the two `check`s and the identical refusal for both taps, and `#print axioms` gives Lean's three for `dont_go_wrong` and `never_wrong`. `lake build` is green.

## Direction

Prove the corollary over `Reachable`: for `check p = true`, a refusal at one of these checks implies a value holding a number that is not finite. Or have these checks report a value that fails `typed` (`Runtime.lean:122`, `conforms` without finiteness) as `Err.type`, which `Legitimate` already leaves out, so the theorem itself has to rule it out.

## Related

`QUEUE.md`, "A Lean theorem for LLP 1089 D3" and "One-slot invariants of a queue mutation's slot", are other theorems.
