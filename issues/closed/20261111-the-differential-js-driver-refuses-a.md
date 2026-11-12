# The differential JS driver refuses a valid select change when option text has whitespace

**Status:** Closed
**Resolution:** The driver reads an option without a `value` attribute as its text with HTML's ASCII whitespace stripped and collapsed, as the kernel's `option_label` does, and a corpus case covers padded, tabbed, inner-run and nonbreaking-space labels.
**Systems:** semantics/difftest/js/drive.mjs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, the select delivery check compares an option's raw `textContent` with the incoming value when no value attribute exists (`semantics/difftest/js/drive.mjs:299`). HTML option values collapse HTML whitespace. A browser can deliver `b` for text `  b  `, but the differential driver calls it refused.

## The failure

```contract
component App
  state chosen = "a"
  action choose(value: string)
    chosen = value
  view
    select testId="select" value=chosen change=choose
      option "a"
      option "  b  "
```

Drive `{ "type": ["select", "b"] }`. Expected: `outcome ok` and `slot chosen "b"`. Observed: `outcome refused` and `slot chosen "a"`.

## Verification

Ran `bun semantics/difftest/js/drive.mjs /tmp/exact-fix04-select-batch.json` with the current `target/debug/exact-web-js` compiler, the temporary Contract above beside the driver, empty answers/facts/hostSources, and the one type event. It printed:

```
== type "select" "b"
outcome refused
slot chosen "a"
```

A separate headless Chrome probe through `suiteBrowser` read `{textContent: "  b  ", value: "b"}` from that option. The temporary Contract and script were removed.

## Direction

Use the option value semantics in the driver: explicit value if present, otherwise descendant text with HTML whitespace collapsed. Cover both a padded label and a nonbreaking space.

## Related

`issues/closed/20261110-document-select-option-without-value-not-selected.md` repaired the document renderers' fallback values. The driver's delivery check remains separate. No matching open issue or QUEUE entry was found.
