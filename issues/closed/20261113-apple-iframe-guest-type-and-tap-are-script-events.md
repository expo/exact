# On Apple, the agent's `type` and `tap` into an iframe guest are script events that do not type or click as a person does

**Status:** Closed
**Resolution:** The guest `type` and `tap` scripts (`WebGuestInput.swift`) insert text at the caret with `execCommand('insertText')` in an editable (setting `value` only on a field), choose the first shown editable, put a chord's modifiers in the KeyboardEvent's fields, and place focus and the caret at a press's point.
**Systems:** host/apple/Sources/ExactKit/WebModule.swift (`tap`, `type`), host/apple/webarm/FrameAutomation.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`WebModule.type` and `WebModule.tap` (`WebModule.swift:821` and `:791`) drive a guest
page by evaluating script in it. LLP 1020 D4 makes that input untrusted on purpose
(`isTrusted: false`). Four parts of it differ from a person's input and keep a rich-text
guest from being driven:

- **Text replaces the editable.** For a target without `value`, `type` sets
  `target.textContent = text` (`:830`), so on a `contenteditable` it replaces the whole
  editable instead of inserting at the caret, then sends a synthetic `input` event.
- **The target can be hidden.** With no selector and nothing focused, `type` takes
  `document.querySelector('input,textarea,[contenteditable]')` (`:837`): the first in DOM
  order, even one that is hidden.
- **A chord becomes the key name.** `type <frame> key Meta+s` dispatches
  `new KeyboardEvent('keydown', {key: 'Meta+s'})` (`:828`): the whole chord string is the
  `key` and no `metaKey` is set, so no page handler matches. A key without modifiers
  (`Escape`) works.
- **A click places no caret.** `tap <frame> at x y` dispatches untrusted `pointerdown`,
  `pointerup` and `click` (`:804-806`). WebKit places no caret and makes no selection for
  them.

The partitioned frame's automation already does the first and third properly:
`FrameAutomation.swift:694-700` splits modifiers into `shiftKey`, `ctrlKey`, `altKey` and
`metaKey`, and inserts text with `document.execCommand('insertText')` at the selection.

## The failure

On Exact `5434f432`, macOS 26.6, an app embedding a bundled rich-text editor page in an
iframe: the guest could be read (`tree` lists its elements) but not edited. `type <frame>
<text>` replaced the editor's content, `type <frame> key Meta+s` reached no shortcut
handler, and `tap <frame> at x y` put no caret in the text. Guest elements that `tree`
lists cannot be named as targets either.

## Fix

Bring `WebModule.type` to what `FrameAutomation` does: parse the chord into the
`KeyboardEvent`'s modifier fields, insert text with `execCommand('insertText')` at the
selection (setting `value` only for `input` and `textarea`), and skip hidden candidates
when choosing a target. For a caret at a point, `tap … at` could set the selection with
`document.caretPositionFromPoint` before its events, or the Apple host could click the
`WKWebView` with a real `NSEvent`/`UITouch` at that point, as it does for page nodes.
