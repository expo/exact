# Uncaught `NSRangeException` crash in iOS multi-paragraph `SelectionDocument.report` and `TextSelectionHost.selectWord` on Markdown paragraphs

**Status:** Open
**Systems:** ExactTextSelection (iOS) — SelectionDocument, TextSelectionHost
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Copy-paste drift)

Verification: verified by reading.

At `9c0431fd5`, two iOS multi-paragraph text-selection paths slice `node.paragraphText` using un-clamped shaped offsets from `SelectionDocument`:

- `host/apple/Sources/ExactTextSelection/TextSelectionDocumentIOS.swift:130-140` (`SelectionDocument.report`):
  ```swift
  let lo = selected.length > 0 ? part.node.sourceOffset(selected.location - part.start) : 0
  let hi = selected.length > 0 ? part.node.sourceOffset(NSMaxRange(selected) - part.start) : 0
  let local = NSRange(location: lo, length: hi - lo)
  guard reported[part.node.id] ?? NSRange(location: 0, length: 0) != local else { continue }
  reported[part.node.id] = local
  host.report(part.node, (part.node.paragraphText as NSString).substring(with: local), local)
  ```
- `host/apple/Sources/ExactTextSelection/TextSelectionIOS.swift:112-118` (`TextSelectionHost.selectWord`):
  ```swift
  let lo = node.sourceOffset(selected.location - part.start), hi = node.sourceOffset(NSMaxRange(selected) - part.start)
  let range = NSRange(location: lo, length: hi - lo)
  return ((node.paragraphText as NSString).substring(with: range), range)
  ```

### Reachable trigger

On iOS, render a container with `user-select: text` holding two or more `<text>` paragraphs (none with a context menu), so `TextSelectionHost.install` builds a shared `SelectionDocument` (`TextSelectionIOS.swift:71, 78-83`). Give at least one paragraph `markup="markdown"` with an inline atom (e.g., `props["text"] = "Hi @a"`, UTF-16 length 5):

1. `SelectionDocument.init` (`TextSelectionDocumentIOS.swift:47-48`) sets `part.length = SelectableText.shaped(node).length` (`TextSelectionIOS.swift:185`: `node.paragraphSpec().runs.map(\.text).joined() as NSString`).
2. `NodeText.swift:188-197` expands Markdown via `MarkupRuns.expand` (`MarkupRuns.swift:94-101`), which wraps every inline atom in `\u{202F}\u{2066}<label>\u{2069}\u{202F}` (at least +4 UTF-16 code units even when the label equals the source token, and more when the face label is longer); if the atom has an icon, `NodeText.swift:284-285` -> `TextEngine.atomIcons` (`TextAtomIcon.swift:38-49`) inserts `\u{2060}\u{FFFC}\u{2060}` (+3 more UTF-16 code units). Thus `part.length > (node.paragraphText as NSString).length` (where `paragraphText` returns raw `props["text"]`, `TextInteraction.swift:50-52`).
3. `TextStyle.spec` (`ExactCore/TextStyle.swift:49`) assigns an empty identity `SourceMap()` whenever `props["markup"] == "markdown"`, so `node.sourceOffset(k)` (`NodeText.swift:293` -> `ExactCore/Text.swift:293-297`) returns `k` unchanged.
4. Selecting across the trailing portion of that paragraph when it has a `selectionchange` handler calls `SelectionDocument.report` (`TextSelectionIOS.swift:299` -> `TextSelectionDocumentIOS.swift:138`), or invoking `selectWord` (e.g. via agent `dblclick` / `contextmenu`, `TextSelectionIOS.swift:128-133`) on its trailing text reaches `TextSelectionIOS.swift:117` with `NSMaxRange(local) > (paragraphText as NSString).length` (or `lo > length`).

### Actual vs. expected

- **Actual:** `(part.node.paragraphText as NSString).substring(with: local)` (`TextSelectionDocumentIOS.swift:138`) and `(node.paragraphText as NSString).substring(with: range)` (`TextSelectionIOS.swift:117`) throw an uncaught Objective-C `NSRangeException` (`Range out of bounds`), crashing the iOS app.
- **Expected:** Both sites clamp `lo` and `hi` to `(node.paragraphText as NSString).length` and guard `hi > lo` (reporting an empty range `{0, 0}` / returning `nil` when clamped empty) before slicing `paragraphText`, matching the single-paragraph iOS sibling `SelectableText.reportable` (`TextSelectionIOS.swift:279-283`) and the macOS sibling `TextSelection.report` (`TextSelectionMac.swift:248-250`).

### Verification and siblings checked

- Verified by reading `TextSelectionDocumentIOS.swift`, `TextSelectionIOS.swift`, `NodeText.swift`, `MarkupRuns.swift`, `TextAtomIcon.swift`, `TextStyle.swift`, `ExactCore/Text.swift`, and `TextInteraction.swift`.
- Siblings checked:
  - Single-paragraph iOS `SelectableText.reportable` (`host/apple/Sources/ExactTextSelection/TextSelectionIOS.swift:279-283`) clamps `lo`/`hi` with `min(..., source.length)` and guards `hi > lo`.
  - macOS `TextSelection.report` (`host/apple/Sources/ExactKit/Mac/TextSelectionMac.swift:247-251`) clamps `lo`/`hi` with `min(..., text.length)` and guards `hi > lo`.
  - Painted host (`host/painted/src/presenter/reader_selection.rs:150-152`) clamps `start`/`end` against `own` and skips when `start >= end`.
  - `InlineText.swift:183` and `TextSelectionMac.swift:574` only perform range-safe `NSLocationInRange` checks on `inlineText`; `TextSelectionMac.swift:988, 1010` store `Position.offset` without slicing `NSString`.

### Suggested fix and regression test

- In `SelectionDocument.report` (`TextSelectionDocumentIOS.swift:133-138`), compute `let source = part.node.paragraphText as NSString`, clamp `lo = min(part.node.sourceOffset(...), source.length)` and `hi = min(part.node.sourceOffset(...), source.length)`, and set `local = hi > lo ? NSRange(location: lo, length: hi - lo) : NSRange(location: 0, length: 0)` before calling `source.substring(with: local)`.
- In `TextSelectionHost.selectWord` (`TextSelectionIOS.swift:115-117`), clamp `lo` and `hi` to `source.length`, `guard hi > lo else { return nil }`, and slice `source.substring(with: range)` (or share a clamping helper with `SelectableText.reportable`).
- Add an iOS `ExactTextSelection` test with two paragraphs inside a `user-select: text` container where one paragraph has `markup="markdown"` with an inline atom (longer shaped text than source) and `selectionchange`, verifying both multi-paragraph selection reporting and `selectWord` across the atom do not crash.

### Siblings checked

Single-paragraph iOS SelectableText.reportable (TextSelectionIOS.swift:279-283), macOS TextSelection.report (TextSelectionMac.swift:247-251), and painted host reader_selection.rs:150-152 all clamp mapped source offsets before slicing; InlineText.swift:183 and TextSelectionMac.swift:574,988,1010 do not slice NSString directly.
