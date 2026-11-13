# A Figma pull overwrites edits made after its conflict check

**Status:** Open
**Systems:** tools/figma-tamber/src/code.ts, tools/figma-tamber/src/ui.ts
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

The plugin checks the document for conflicting values, waits for the UI to confirm
any warnings, then sends a separate `apply` message (`ui.ts:68–83`). That message
carries only the exported design. The main thread reads the saved plan and writes
the variables without checking whether their current values still match those
reviewed (`code.ts:297–312`).

A document edit arriving between the check and the write is therefore overwritten
without being included in the conflict prompt. The UI's `busy` flag serializes
plugin commands but does not prevent document changes, including another person's
edits while the confirmation is open. Undo can recover the overwritten value, but
the successful pull gives no indication that it replaced an unreviewed edit.

## Reproduction

Verified at `f6be99af` with the actual plugin main thread and the existing
`code.test.ts` in-memory Figma API:

1. Apply `test/theme-fixture.export.json` to create the variables.
2. Send `check` with the same export: it reports zero conflicts.
3. Change `accent/fill` in the default mode to opaque red.
4. Send `apply` with the checked export.

The reply is `applied`, and the red value becomes the export's blue
`#1d9bf0`. This verifies the message transition; it was not a live Figma run.

## Fix

Tie an apply to the document values and structure that were checked. Refuse a
changed document and repeat the check before writing. Include changes during
asynchronous API reads, and keep rollback scoped to the plugin's own edits.

## Existing reports

No matching filesystem issue or GitHub issue was found in the review's all-state
snapshot of `expo/exact` issues.
