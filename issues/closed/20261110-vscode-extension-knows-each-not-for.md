# The VS Code extension highlights and indents the retired `each`, and does neither for `for`

**Status:** Closed
**Resolution:** The grammar's control keywords and the indentation rule name `for` where they named the retired `each`.
**Systems:** editors/vscode/syntaxes/exact-contract.tmLanguage.json, editors/vscode/language-configuration.json
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A view repeats with `for x in xs key=x.id`. The older word, `each`, is refused with `syntax-view-keyword` (`contract/syntax/src/parser.rs:1264-1267` at `dd6f5dcd`), and `for` is one of the sixteen reserved words (`contract/syntax/src/parser/names.rs:20-23`). The extension, whose README says it covers "the current `.contract` language", still has the old word in both places it names control keywords:

- the grammar's control keywords (`exact-contract.tmLanguage.json:305`): `when|if|else|each|in|key|match|case|as|shape|mount|from|to|then|refreshes|with|children`. The word `for` is nowhere in the file, so it is scoped as an ordinary name;
- `increaseIndentPattern` (`language-configuration.json:88`): `…|when|if|else|each|match|case|provide`. Enter after a `for` line does not indent, though its body must be indented.

The test that keeps the grammar in step with the compiler (`contract/cli/tests/it/vscode_grammar.rs`) compares tags, events and host commands only, so nothing fails.

## The failure

```
component Main
  state items: string[] = ["a", "b"]
  view
    column
      for item in items key=item
        text item
```

Observed: the only pattern of the grammar that matches `for` is the one for an ordinary name (`variable.other`), and the indentation rule does not match the `for` line. Written `each item in items key=item`, the line is highlighted as a keyword and indents, and `contract build` refuses it:

```
each.contract:5:7 [syntax-view-keyword] a view repeats with `for`: write `for x in xs key=x.id`
```

Expected: `for` scoped `keyword.control` and matched by the indentation rule; `each` in neither.

## Verification

At `dd6f5dcd`, ran every `match` and `begin` pattern of the grammar and the indentation pattern, as JavaScript regular expressions (as VS Code runs the indentation rule), over the two lines, and built both spellings:

```
grammar: `for` in "    for item in items key=item.id" → ["variable.other.exact-contract"]
grammar: `each` in "    each item in items key=item.id" → ["keyword.control.exact-contract","variable.other.exact-contract"]
increaseIndentPattern DOES NOT indent after "    for item in items key=item.id"
increaseIndentPattern indents after "    each item in items key=item.id"
contract build for.contract: exit 0
contract build each.contract: exit 1
```

Not run: the extension inside VS Code. Its TextMate engine is Oniguruma; these patterns use nothing the two engines read differently.

## Direction

Replace `each` with `for` in both lists.

## Related

`when` stays in both lists: sequences and tools still use it (`parser/sequence.rs:15`, `parser/tool.rs:66`), though a view's `when` is refused as `each` is (`parser.rs:1260-1263`). No issue or QUEUE entry mentions the grammar's keywords.
