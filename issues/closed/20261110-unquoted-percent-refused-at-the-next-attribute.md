# `width=50%` is refused as a missing expression, at the next attribute

**Status:** Closed
**Resolution:** An attribute, style or keyframe value that is a number and `%` with nothing after it on the line but another attribute is refused as `syntax-unquoted-length` at the number and the `%`, with the quoted spelling; a remainder with its operand is unchanged.
**Systems:** contract/syntax (parser, attribute values)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A length with a unit is a string in Contract: `docs/start-here.md:208` says "Quote
units (`width="50%"`); a bare number is pixels", and `docs/contract-grammar.md:51`
"Units belong in quoted strings." The parser catches the unquoted form when the
unit is a word: `width=10px` lexes as `10` and a name written against it, and
`unquoted_unit` refuses it with the quoted spelling
(`contract/syntax/src/parser.rs:1398-1415`, `syntax-unquoted-length`). The other
unit a CSS author writes, `%`, is the remainder operator
(`contract/syntax/src/parser/expr.rs:117`), so `50%` starts a binary expression
whose right operand is whatever follows. The refusal comes from wherever that
operand fails to parse, before `unquoted_unit` runs, and says nothing about
quoting.

## The failure

```
component App
  state a = 1
  view
    column width=50% testId="c"
      text `${a}`
```

`contract build` prints:

```
app.contract:4:28 [syntax-expected-expression] expected an expression, found `=`
 4 |     column width=50% testId="c"
   |                            ^
```

The caret is under the `=` of `testId=`, ten columns past the mistake, and the
message describes `50 % testId`. Expected, as `width=10px` gets at `10px`:

```
[syntax-unquoted-length] `width=50%` needs quotes: a value with a unit is a string, `width="50%"` (a bare number is pixels)
```

## Verification

Ran `contract build` (debug build) on the program above and on variants of line 4:

| Line 4 | Result |
| --- | --- |
| `column width=50% testId="c"` | as above, 4:28 |
| `column width=50%` | `4:21 [syntax-expected-expression] expected an expression, found end of line` |
| `width=50%` as a row of a `style` block | the same, at the end of its line |
| `column width=10px testId="c"` | ``4:18 [syntax-unquoted-length] `width=10px` needs quotes: … `` |
| `column width="50%" testId="c"` | compiles |
| `column width=a % b testId="c"` (two number states) | compiles |

## Direction

In `attr_value`, when an attribute's value is a number literal directly followed
by `%` and then by a `name=` pair or the end of the line, refuse with
`syntax-unquoted-length` at the number and the `%`. A remainder as an attribute's
whole value (`width=a % b`) has both operands and is untouched.

## Related

`QUEUE.md`, "CSS values builders reach for and Contract refuses" lists values the
kernel refuses, not this spelling. `docs/agent-pitfalls.md:663` lists
`syntax-unquoted-length` for a keyframe's `rotate=-2deg`, not `%`.
