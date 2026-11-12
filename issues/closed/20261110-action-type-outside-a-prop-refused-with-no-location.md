# An `action` type outside a prop passes the type checker and is refused by lowering at 0:0

**Status:** Closed
**Resolution:** The type pass refuses `action` written as the type of anything but a prop or an inject, and an action held in a list or `some(…)`, as `type-action-held` at the type or the value, naming the declaration and what to write instead.
**Systems:** contract/types, contract/lower
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`Shapes::resolve` turns the written type `action` into `Ty::Action` wherever a type
is written (`contract/types/src/lib.rs:335-339`). Only a prop or an inject can hold
an action (LLP 1006 §2: "a prop of type `action` is an action reference the use
site supplies"), but the type pass does not refuse the word elsewhere: a shape's
field (`lib.rs:1180`), a state's declared type (`lib.rs:689`), a resource's or
mutation's shape (`contract/types/src/component.rs:139-146`), an action's parameter
(`component.rs:211`). An action in a list or in `some(…)` is inferred the same way
(`lib.rs:738-758`). The program reaches lowering, whose type table has no row for
an action and refuses with an empty span (`contract/lower/src/lib.rs:922-927`,
`Span::default()`).

## The failure

A menu whose rows carry their handler, as a React author would write it:

```
shape MenuItem
  label: string
  run: action

component App
  state n = 0
  view
    column
      text `${n}`
```

`contract build` prints:

```
app.contract:0:0 [lower-unlowerable-type] `action()` has no plan type
```

There is no line, no excerpt and no word about what to write; `--json` carries
`"line":0,"col":0`. The same refusal comes from each of these alone in an
otherwise valid app:

```
run: option<action>                    // a shape's field
state f: option<action> = none
mutation m as shape action
resource r = load() as shape action
action run(f: action)                  // never bound
derive fs = [bump]                     // bump is an action
derive f = some(bump)
```

Expected: a `type-` refusal at the written type (or the value) saying that an
action is passed only as a prop or an inject and bound by a handler, as
`lower-action-as-value` says for `text bump` ("`bump` is an action, not a value",
with its line and column).

## Verification

Ran `contract build` (debug build) on the program above and on one program per
line of the list; each printed the `0:0` line and exited 1. An `action` prop on a
child component, passed `go=bump` and bound by `press=go`, compiles.

## Direction

Refuse `action` at the type's own span in every caller of `Shapes::resolve` other
than props and injects, and refuse an action as a list item or inside `some(…)`
in `infer`. Lowering's arm is then unreachable for an authored program. A `fn`
parameter typed `action` compiles today and can be passed nothing (`pick(bump)`
is `lower-action-as-value`); refuse it with the rest.

## Related

`lower-action-as-value` covers an action read as a value in an expression; this
is the type written, or the value held in a list or an option.
`issues/closed/20260901-action-prefix-type-typos.md` is about names that start
with `action`. Nothing in `QUEUE.md` or `docs/agent-pitfalls.md` names
`lower-unlowerable-type`.
