# Changing a search input's appearance drops its autocomplete type

**Status:** Closed
**Resolution:** A field remade for a new class sets what its props say of it (its autocomplete content type, enabled and editable state, and placeholder) before it takes the old field's place, so a style-only `appearance` change keeps them.
**Systems:** host/apple/Sources/ExactKit/Mac/NativeFieldsMac.swift, host/apple/Sources/ExactKit/Mac/NodeViewMac.swift
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/apple/Sources/ExactKit/Mac/NativeFieldsMac.swift:86–99` replaces a search field with a plain field when `appearance` becomes `none`, copying its value, font and color but not its `contentType`. The style path at `host/apple/Sources/ExactKit/Mac/NodeViewMac.swift:1232` does not run the props assignment at line 1025, which normally applies `autocomplete`.

## The failure

Create `input type="search" disabled=true autocomplete="email"`, then apply only `appearance: none`. The replacement field has `contentType == nil` instead of `.emailAddress`. This loses the autocomplete metadata even though the author did not change it. The reported enabled-state change did not reproduce: the replacement remained disabled.

## Verification

Ran `bun host/apple/build.mjs --test --only Fix08ReviewMacTests`. The temporary XCTest used an actual Exact session and applied create and style batches. It printed `before style: class=SearchField enabled=false editable=false contentType=Optional(...email)` and `after style: class=Field enabled=false editable=true contentType=nil`. The class and disabled-state assertions passed; the `.emailAddress` assertion failed. The temporary test was removed.

## Direction

Initialize all property-derived field state when replacing the control during a style update, including its autocomplete content type and editability.

## Related

Reported in the p2-focused-input-type-change review. The related [focused field replacement issue](closed/20261110-focused-input-type-change-blurs-and-selects-all.md) covers focus and selection, not property loss on a style-only replacement.
