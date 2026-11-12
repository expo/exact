# An alert input changed to password keeps its plain text field

**Status:** Closed
**Resolution:** A prompt input whose type changes to or from `password` while its alert shows gets a field of the new class in the accessory view, keeping its value, editing, selection, width and first responder.
**Systems:** host/apple/Sources/ExactKit/Mac/NativeFieldsMac.swift, host/apple/Sources/ExactKit/Mac/AlertsMac.swift
**Severity:** P2
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/apple/Sources/ExactKit/Mac/NativeFieldsMac.swift:83` skips field replacement when `pageField != nil`. An alert prompt lends its field at line 300. `host/apple/Sources/ExactKit/Mac/AlertsMac.swift:235–245` validates field identity without validating its AppKit class, so the alert survives a change to `type="password"` with the original plain field.

## The failure

Open an alert prompt with a search input, focus its accessory field, then apply a props batch setting `type="password"` and `value="secret"`. The lent field remains `Field` with the readable value `secret`. It should become `NSSecureTextField` while preserving editing and selection.

## Verification

Ran `bun host/apple/build.mjs --test --only Fix08MacTests,Fix08AlertMacTests,Fix08HeaderMacTests`. The temporary alert test opened an actual NSAlert sheet, focused its field and applied the props batch. It printed `FIX08 lent prompt: type=password class=Field lent=true value=secret`; the secure-field assertion failed. The toolbar control case passed: `FIX08 toolbar: type=password class=SecureField lent=false`. This verifies the alert case, not the reported toolbar case. The temporary tests were removed.

## Direction

Replace a lent alert field when its required class changes, preserving its value, focus and selection and updating the accessory view's ownership.

## Related

[Focused input type change blurs and selects all](closed/20261110-focused-input-type-change-blurs-and-selects-all.md) covers page field replacement. The early return for lent alert fields remains. Reported in the p2-focused-input-type-change review.
