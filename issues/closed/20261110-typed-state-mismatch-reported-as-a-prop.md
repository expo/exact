# A state whose initializer does not fit its declared type is refused as a prop's argument

**Status:** Closed
**Resolution:** A state whose initializer does not fit its declared type is refused as `type-state-initializer` at the initializer, naming the state, both types and a rejected enum member; the repeated refusal of a use's argument noted under Verification is left as it was.
**Systems:** contract/types (`ascribe`), contract/syntax (parser, `Expr::Typed`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`state x: T = e` (LLP 1116.015 D2) is parsed as `Expr::Typed(e, T, span)` with the
`state` keyword's span (`contract/syntax/src/parser/component.rs:280-283`).
`Expr::Typed` was made for a use's argument typed by the prop it fills
(`contract/syntax/src/ast.rs:1157-1161`), and its checker, `ascribe`, words every
mismatch that way (`contract/types/src/lib.rs:696-700`):

```rust
false => err(
    "type-prop",
    format!("this argument is `{t}`, where the prop is declared `{declared}`"),
    span,
),
```

A state reaches the same code through `infer` (`lib.rs:1060-1063`), so its refusal
names an argument and a prop the program does not have, under the id `type-prop`,
with the caret under the word `state`.

## The failure

```
component App
  state n: number = "a"
  view
    column
      text `${n}`
```

`contract build` prints:

```
app.contract:2:3 [type-prop] this argument is `string`, where the prop is declared `number`
 2 |   state n: number = "a"
   |   ^^^^^
```

Expected: a refusal that says the state `n` is declared `number` and its
initializer is `string`, at the initializer, as an assignment's does. `n = "a"` in
an action prints ``4:5 [type-assign] `n` is `number`, cannot assign `string` ``.

## Verification

Ran `contract build` (debug build) on the program above. The same wording comes
from `state n: option<number> = some("a")` ("this argument is `option<string>`,
where the prop is declared `option<number>`") and from `state xs: Item[] = [1]`
("this argument is `number[]`, where the prop is declared `Item[]`"), both at
the word `state`. `state n: option<number> = none` compiles.

For a use, `ascribe` repeats a refusal already made. `Badge(count=none)` against
`count: number` printed two, at the name and at the value:

```
5:13 [type-prop] `count` expects `number`, given `option<?>`
5:19 [type-prop] this argument is `option<?>`, where the prop is declared `number`
```

`Badge(count="a")` printed only the first.

## Direction

Tell a state's `Expr::Typed` apart from a use's argument (the state's name is at
hand in `component.rs:280`), or check a state's declared type where states are
inferred and word the refusal there, at the initializer's span.

## Related

`QUEUE.md`, "`contract fmt` refuses a typed state" is the formatter's handling of
the same declaration. Nothing in `issues/` or `docs/agent-pitfalls.md` quotes this
message.
