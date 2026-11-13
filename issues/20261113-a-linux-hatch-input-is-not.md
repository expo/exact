# A Linux hatch input is not a replacement

**Status:** Open
**Systems:** host/linux, runner, host/web, host/apple
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

A hatch's `input` replaces a field's whole value. The grammar says that event's `inputType` is `insertReplacementText`, and that this type describes hatch replacements. The web hatch dispatches that type. Apple sends the replacement input (kind 54). The Linux presenter sends `ControlValue::Field`.

The runner sets `insertReplacementText` only for `ControlValue::Replacement`. A `Field` input records an empty type. A handler that follows the grammar therefore treats a Linux hatch write as an ordinary input, and a web or Apple hatch write as a replacement.

## Where

`host/linux/src/presenter/hatch.rs`, the `ActKind::Input` arm. `runner/src/runner/event.rs`, the replacement flag. `runner/src/runner/field.rs`, `input_record`. `host/web-js/hatches.js`, the hatch `input` event. `host/apple/Sources/ExactCore/FieldWire.swift` and `HeadlessSession.swift`, `replaceText`. `docs/contract-grammar.md`, the `setRangeText` deviation that names hatch replacements.

## How to reproduce

Give a field an `input` handler that records `inputType`. On Linux, have a hatch call `input` with new text. The event's type is empty. The same hatch on the web or on Apple reports `insertReplacementText`.

## Direction

Dispatch the Linux hatch write as `ControlValue::Replacement`, with the caret at the end of the new value.
