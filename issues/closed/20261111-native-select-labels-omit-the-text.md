# Native select labels omit the text of an option’s descendants

**Status:** Closed
**Resolution:** A select's option label reads the text of the option and its descendants, as its value does, each node's text as its `text-transform` shows it, whitespace collapsed.
**Systems:** kernel/src/control.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, `Kernel::select_choices` forms a label from only `option.text_runs()` (`kernel/src/control.rs:443`). Its fallback value takes the node's own text and descendant text through `option_value` (`:460`). A native menu's label can therefore disagree with its value and with the browser's option label.

## The failure

A select with an option whose own text is `Dark` and whose child text is `er`, with no explicit value, produces:

```
option label="Dark", value="Darker"
```

Expected: both are `Darker`. The menu label returned to native hosts loses the suffix.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p exact-kernel --test review_findings -- --nocapture`. A temporary test created the select and text nodes with `Kernel::apply`, marking the option with `SemanticTag="option"`, then called `select_choices`. Its `label == "Darker"` assertion failed with `left: "Dark", right: "Darker"`. The existing `option_values_include_own_text_and_descendants` test at `kernel/src/control.rs:817` checks only the value. The temporary test was removed; native menus were not driven.

## Direction

Collect descendant text for the label too, preserving the label's text-transform behavior and HTML whitespace collapse.

## Related

`issues/closed/20261110-document-select-option-without-value-not-selected.md` concerns selection in rendered documents, not native menu labels. No matching issue or QUEUE entry was found.
