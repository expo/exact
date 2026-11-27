# Uncaught `NSRangeException` crash and `disabled`/`inert` focus bypass in `TextArea.pressAtom` on macOS and iOS

**Status:** Closed
**Resolution:** Validate disabled, inert, isSelectable, and atom range bounds before and after focus in TextArea.pressAtom, gate pointer/accessibility/agent atom press paths, and sync macOS TextArea.isSelectable in InputGates.refresh.
**Systems:** ExactKit (macOS & iOS) — TextArea inline atoms & AtomElement accessibility
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `TextArea.pressAtom(_:held:)` (`host/apple/Sources/ExactKit/FieldAtoms.swift:64-75`) indexes into `NSTextStorage` without validating bounds and mutates first-responder focus and selection before checking `disabled`, `inert`, or `isSelectable`:

```swift
guard let owner, let storage = atomStorage,
      let atom = storage.attribute(.markupAtom, at: range.location, effectiveRange: nil) as? MarkupAtom else { return }
#if os(macOS)
if window?.firstResponder !== self { window?.makeFirstResponder(self) }
setSelectedRange(range)
#else
if !isFirstResponder { _ = becomeFirstResponder() }
selectedRange = range
#endif
let source = (storage.string as NSString).substring(with: range)
owner.pressAtom(kind: atom.kind, source: source, href: atom.href, range: range, held: held)
```

### Reachable triggers

1. **Stale `AtomElement` range (macOS & iOS — crash):** `AtomElement` stores an immutable `let range: NSRange` (`FieldAtoms.swift:330`, `415`) and forwards it directly from `accessibilityPerformPress()` (`FieldAtoms.swift:348-352`) and `accessibilityActivate()` (`FieldAtoms.swift:431-435`). Because `markdownRestyle()` refreshes `atomElements` asynchronously on the next main-queue turn (`host/apple/Sources/ExactMarkdown/MarkupEditorMac.swift:26`, `MarkupEditorIOS.swift:24`) and accessibility clients retain element handles across edits, activating an `AtomElement` after the textarea's value shrinks passes an out-of-bounds `range` to `pressAtom`.
2. **Synchronous `focus` / `focusin` / `focusout` mutation (macOS & iOS — crash):** Clicking or tapping an inline atom in an unfocused `<textarea markup="markdown">` calls `window?.makeFirstResponder(self)` (`FieldAtoms.swift:67`) or `becomeFirstResponder()` (`FieldAtoms.swift:70`). `TextArea.becomeFirstResponder()` (`TextAreaMac.swift:145-153`, `TextAreaIOS.swift:367-376`) calls `presenter.focus(owner.id)` -> `FocusEvents.focus` (`FocusEvents.swift:135-146`) -> `presenter.afterBatch`, which runs synchronously when `!applying` (`PresenterMac.swift:991`, `PresenterIOS.swift:849`), dispatching `runtime.focus` and applying the returned batch (`Session.swift:835`). If the `focus`/`focusin` handler (or the outgoing element's `focusout` handler) clears or shortens the textarea's `value`, `writeValue` mutates `storage` synchronously before `setSelectedRange(range)` / `selectedRange = range` (`FieldAtoms.swift:68, 71`) and `(storage.string as NSString).substring(with: range)` (`FieldAtoms.swift:73`) run with the stale `range`.
3. **`disabled` / `inert` / modal-blocked focus and selection bypass (macOS & iOS):** `pressAtom` checks none of `!owner.disabled`, `!owner.inert`, `isSelectable`, or `atomsAccessibilityExposed`. On macOS, `InputGates.refresh` (`InputGatesMac.swift:95-98`) clears `area.isEditable` when `inert` is true but leaves `area.isSelectable` true, so activating an `AtomElement` while the textarea is `inert` or behind a modal dialog steals first responder and mutates selection before `NodeView.pressAtom` (`FieldAtoms.swift:442`) drops dispatch. On iOS, `NodeView.applyProps` (`NodeViewIOS.swift:1095-1099`) only toggles `isUserInteractionEnabled` for `inert` (not `disabled`), and `AtomTap` (`FieldAtoms.swift:359-373`) never checks `field.isSelectable`, `!owner.disabled`, or `!owner.inert`, so tapping an atom on a disabled textarea — or activating `AtomElement` on a disabled/inert/modal-blocked textarea — mutates selection (and on inert/modal-blocked fields steals focus) before line 442 rejects dispatch.

### Actual vs. expected

- **Actual:** Paths (1) and (2) throw an uncaught Objective-C `NSRangeException` in `storage.attribute(.markupAtom, at: range.location, effectiveRange: nil)`, `setSelectedRange(range)`, or `(storage.string as NSString).substring(with: range)`, crashing the app (or slicing non-atom text if an atom shrank in place). Path (3) steals focus and mutates selection on disabled, inert, or modal-blocked text areas.
- **Expected:** `pressAtom` rejects disabled/inert/non-selectable fields and validates that `range` is in bounds and still matches the live atom span before focusing, selecting, or slicing — and re-validates bounds and atom range after `makeFirstResponder`/`becomeFirstResponder`. This matches `NSAttributedString.atomRange(at:)` (`MarkdownLink.swift:215-219`), `AtomTap`'s own async re-check (`FieldAtoms.swift:370`), `TextArea.atomHit` / `pressAtom(with:)` (`FieldAtoms.swift:184, 192`), `NodeView.pressAtom` (`FieldAtoms.swift:442`), and Foundation's preconditions for `NSAttributedString.attribute(_:at:)` and `NSString.substring(with:)`.

### Verification and siblings checked

- Verified by reading the call chains across `FieldAtoms.swift`, `MarkupEditorMac.swift`, `MarkupEditorIOS.swift`, `TextAreaMac.swift`, `TextAreaIOS.swift`, `FocusEvents.swift`, `Session.swift`, `InputGatesMac.swift`, and `NodeViewIOS.swift`.
- Siblings checked:
  - `ReaderAtomAccessibility` (`FieldAtoms.swift:594-599` macOS, `623-626` iOS) already guards `!owner.disabled, !owner.inert, owner.handlers.contains("atompress")` and does not slice `NSTextStorage`.
  - `AgentPanIOS.swift:288` calls `area.pressAtom(hit.range)` (guarded by `!node.disabled` at line 270, not `isSelectable`/`inert`); guarding inside `TextArea.pressAtom` protects it too.
  - `host/web/markup-editor.js:746` gates atom clicks with `!atom || destroyed || !pressable()`.

### Suggested fix and regression test

- In `TextArea.pressAtom(_:held:)`: guard `let owner, !owner.disabled, !owner.inert, isSelectable, let storage = atomStorage`, check `storage.atomRange(at: range.location) == range` (which checks `0 <= loc < length` and exact span) and `NSMaxRange(range) <= storage.length` before taking first responder; after `makeFirstResponder`/`becomeFirstResponder`, re-verify `!owner.disabled, !owner.inert, isSelectable, let storage = atomStorage, storage.atomRange(at: range.location) == range, NSMaxRange(range) <= storage.length` and re-read `atom` before calling `setSelectedRange`/`selectedRange` and `substring(with:)`. Also gate `AtomTap.gestureRecognizerShouldBegin`/`tapped`, `AtomElement.accessibilityPerformPress()`/`accessibilityActivate()` (returning `false` when disabled/inert/unexposed or stale), and `InputGates.refresh` (`area.isSelectable = !node.disabled && !inert` on macOS).
- Add unit tests in `InlineAtomEditorTests.swift` covering: (1) calling `accessibilityPerformPress()`/`accessibilityActivate()` on a retained `AtomElement` after clearing/shortening the textarea value; (2) pressing an atom on an unfocused textarea whose `focus` handler clears `value`; (3) activating an atom while `disabled` or `inert` without stealing first responder or changing selection.

### Siblings checked

ReaderAtomAccessibility (FieldAtoms.swift:594-626) already checks !disabled && !inert and uses captured run URLs; MarkdownLink.swift:215-219 (atomRange) checks bounds; AgentPanIOS.swift:288 calls area.pressAtom(hit.range) and benefits from gating inside pressAtom; InputGatesMac.swift:95-98 omits clearing area.isSelectable when inert; host/web/markup-editor.js:746 gates on pressable().
