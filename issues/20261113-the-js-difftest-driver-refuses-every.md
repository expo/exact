# The JS difftest driver refuses every radio type event

**Status:** Open
**Systems:** semantics
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`deliverable` requires the control's type to be neither `checkbox` nor `radio`. A checkbox has no text payload. A radio's change is its own value: the `value` attribute, or `"on"` when it has none. The runner accepts that text and rejects any other. The driver marks every radio `type` refused and does not fire `input` or `change`.

A script that types a radio's own value is `outcome ok` on the runner and `outcome refused` on the JS drive.

## Where

`semantics/difftest/js/drive.mjs`, `deliverable`. `runner/src/runner/control.rs`, the radio arm of `control_payload`.

## How to reproduce

In a `--js` difftest, type a radio's own value into that radio. The runner delivers the change. The JS drive refuses it.

## Direction

Deliver a radio `type` when the text equals that radio's value, and refuse any other text. Keep refusing text on a checkbox.
