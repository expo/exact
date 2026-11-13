# `contract vocab` lists `wrap-flow` and `user-select` values the compiler refuses, and its `--json` form gives `buttonStyle` no names

**Status:** Closed
**Resolution:** The schema marks the enum keywords the wire keeps and authoring refuses (`refused`, read by the compiler and `contract vocab` alike), so `vocab` lists `wrap-flow` and `user-select` as the compiler admits them and names the refused ones, and `--json buttonStyle` lists its twelve names.
**Systems:** contract/cli/src/vocab.rs, contract/lower/src/values.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`contract vocab <name>` is where the guides send an author for what an attribute takes
(`AGENTS.md`: it "answers for any tag or property"). For three attributes its answer and the
compiler disagree:

| Attribute | `contract vocab` says | The compiler |
|---|---|---|
| `wrap-flow` | `enum auto\|both\|start\|end\|minimum\|maximum\|clear` | refuses `start`, `end`, `minimum`, `maximum` and `clear` on every element (`contract/lower/src/values.rs:533-534`) |
| `user-select` | `enum auto\|none\|text\|all\|contain` | refuses `contain` on every element (`values.rs:525-526`) |
| `buttonStyle`, `--json` only | `"type": "str"`, no names | refuses any string but twelve names (`lower-button-style`), which the text form lists |

The first two are the kernel schema's keywords (`row.enum_names()`), printed without the test
lowering applies (`contract/cli/src/vocab.rs:1113` for the text, `:953` for `--json`). The
third is a case `attr_detail` has (`vocab.rs:1193-1197`) and `attr_json` lacks
(`vocab.rs:1047-1049` sets only the note).

## The failure

```
$ contract vocab wrap-flow
wrap-flow: style attribute
  rows wrap_flow
  enum auto|both|start|end|minimum|maximum|clear, default auto
```

```
component A
  view
    column position="relative"
      text "x"
      box position="absolute" wrap-flow="start" width=40 height=40
```

`5:31 [lower-attr-value] unsupported `wrap-flow` value: CSS Exclusions defines it; exact2 v1
implements `both` (or `auto`)`.

`contract vocab user-select` prints `enum auto|none|text|all|contain, default auto`, and
`text "x" user-select="contain"` is `lower-css-user-select` ("Supported values are auto, none,
text and all").

`contract vocab --json buttonStyle` prints an attribute object whose keys are `kind`, `name`,
`note`, `prop` and `type` (`"str"`). `button "Go" press=go buttonStyle="primary"` is
`lower-button-style`, whose message lists the twelve styles.

Expected: both forms list what an app may write, or name a refused keyword as refused, as the
`color-scheme` note does for `normal` (`vocab.rs:913`).

## Verification

Ran each command above with `target/debug/contract` and built each snippet with `contract
build`; the output is quoted as printed. A temporary test wrote each of the 466 keywords
`contract vocab --json` lists for a one-row style attribute on a `box` and compiled it. 78
were refused: 71 for where they were written (the wrong element, or `wrap-flow="both"` without
`position="absolute"`), which the refusal says, and `color-scheme="normal"`, which its note
covers. The other six are the five `wrap-flow` values and `user-select="contain"`; each is
still refused where the attribute is admitted (`wrap-flow` beside `position="absolute"`,
`user-select` on a `text`).

## Direction

Print a row's keywords through the test `values.rs` applies, in `row_text` and `row_json`, or
say the refused ones in the attribute's note. Give `attr_json` the `buttonStyle` names as
`values`.

## Related

`issues/20261110-vocab-json-refuses-headings.md` (`--json` denies `h1`) and
`issues/20261110-vocab-has-no-entry-for-class-part-row-look.md` (three attributes with no
entry) are other wrong answers of the same command. `QUEUE.md`, "CSS native
interaction/painter gaps", owes `user-select: contain` itself.
