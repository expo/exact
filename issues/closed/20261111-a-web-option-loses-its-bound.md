# A web option loses its bound value when its text initially equals that value

**Status:** Closed
**Resolution:** The wasm host's `writeValue` (now `host/web/wasm/views.js`) writes an option's value attribute whenever it differs from the bound value, as the JS target's already did, and both hosts remove the attribute when the binding is cleared, so the label is the value again.
**Systems:** host/web/glue.js
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, `writeValue` returns when `el.value === value` (`host/web/glue.js:404`). On an option without a value attribute, that getter derives its value from its text. The early return skips the assignment that would create the explicit value attribute. The live property writer calls this function for option values (`:453`).

## The failure

Create an option with text `b`, write the authored value `b`, then change only its text to `c`:

```js
option.textContent = 'b';
writeValue(option, 'b');
option.textContent = 'c';
```

Observed: `option.value === 'c'` and `option.getAttribute('value') === null`. Expected: value `b` with attribute `value="b"`; changing the label must not change an explicit bound value.

## Verification

Ran `bun host/web/tests/review-option-value.mjs` in headless Chrome through `suiteBrowser`. The temporary script extracted the current `writeValue` function unchanged and called it on a live option, supplying its two WeakMap/WeakSet holders. It printed:

```json
{"before":{"text":"b","value":"b","attribute":null},"after":{"text":"c","value":"c","attribute":null}}
```

The temporary script was removed. This exercised the writer itself, not a complete app.

## Direction

Handle option values as reflected attributes: write an explicit value even when the derived getter already equals it, and remove that attribute when the binding is cleared.

## Related

`issues/closed/20261110-document-select-option-without-value-not-selected.md` concerns initial document selection. This is a live value write after an option exists. No matching open issue or QUEUE entry was found.
