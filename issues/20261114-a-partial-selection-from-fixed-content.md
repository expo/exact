# a partial selection from fixed content into a virtualized list's rows loses retired rows on copy

**Status:** Open
**Systems:** host/web/list-selection.js, host/web-js/list-text.js, host/web/src/abi.rs
**Author:** Codex, from Astra’s round-7 review of fixed collection content
**Date:** 2026-11-14

## Failure

A partial browser selection from a fixed header into keyed data rows cannot
retain its logical endpoints. Scrolling retires the intervening rows, and
Copy then omits their text. A row-to-fixed-footer selection has the same gap.
Whole-list Select All uses a separate path and does not reproduce this failure.

Astra reported this in round 7 of the fixed-content review. To reproduce:
make a vertical virtualized list with a text header, enough keyed text rows
to retire a window, and a text footer. Select from within the header into a
mounted row, scroll until intervening rows retire, and copy. Compare with
the same selection before scrolling. Repeat from a row into the footer.

## References

- `host/web/list-selection.js`: the selection controller captures keyed
  row endpoints only.
- `host/web-js/list-text.js`: the logical text range accepts keyed row
  endpoints only.
- `host/web/src/abi.rs`: the wasm list-text boundary must carry fixed
  endpoints too.
- `host/apple/Sources/ExactKit/Mac/TextSelectionMac.swift`: `Position`
  and `position(_:offset:)` already distinguish lead/trail paragraph
  endpoints from keyed row endpoints.
- `host/apple/src/abi_collections.rs`: the Apple text-copy ABI carries
  those fixed endpoints.

Preserve fixed-side paragraph and UTF-16 boundaries through the browser
selection controller, JS text projection and wasm ABI. Verify header-to-row
and row-to-footer copies across retirement, in both selection directions.

