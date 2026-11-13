# The iOS sign key bypasses beforeinput and maxlength

**Status:** Closed
**Resolution:** The ± key checks `maxlength` and asks a cancelable `beforeinput` (`insertText` adding the sign, `deleteContent` removing it) before it changes the field, as the field's own edits do.
**Systems:** host/apple/Sources/ExactKit/IOS/PadBarIOS.swift, host/apple/Sources/ExactKit/FieldInput.swift
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the number pad's ± button calls `toggleSign`, which writes
`UITextField.text` directly and calls `fieldChanged`
(`PadBarIOS.swift:77-83`). It does not run `FieldSelections.shouldEdit` or
`nativeEdit`, which dispatch `beforeinput` and honor `preventDefault`
(`FieldInput.swift:70-95`, `:107-115`). It also skips `TextInputLimit.allows`,
which the normal UIKit editing delegate checks before admitting a change
(`IOS/FieldEditingIOS.swift:28-29`).

This is a user edit: the accessory exists because the platform's number
pads lack a minus key, and it reports the changed value through `input`.
An app that rejects an edit in `beforeinput` cannot reject this one, and
its `input` lacks the metadata the shared edit path records. The same
bypass permits a plain text input with `inputmode="numeric"`, `min="-1"`,
`maxlength="1"` and value `"1"` to become `"-1"`. The sign button is
available for plain inputs with a numeric input mode, as well as
`type="number"` (`PadBarIOS.swift:33-42`); the former do enforce
`maxlength` for ordinary typing.

The source trace reaches `fieldChanged` and `typedText` without any
pre-edit decision. `TextField` does not override the `text` setter to
supply one. Existing `PadBarIOSTests.testANegativeMinGetsAPlusMinusKeyThatFlipsTheSignAsInput`
exercises direct sign changes and their input callbacks, but registers no
`beforeinput` handler and supplies no length limit. This report is
source-confirmed; no new UIKit test or simulator run is claimed.

Route the sign change through the shared edit admission path with the
actual replacement range and edit metadata, and enforce text limits before
changing the buffer. Keep the usual checks if a beforeinput handler
removes or replaces the field. Add cases where beforeinput cancels ±,
where adding the sign exceeds maxlength, and where removing an existing
sign succeeds and emits exactly one input.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. Closed
`20261009-field-selection-beforeinput-range-edit.md` records implemented
cancelable beforeinput on plain UIKit inputs; it does not name the number
pad's sign-button path, which still bypasses that implementation.
