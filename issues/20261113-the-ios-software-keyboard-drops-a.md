# The iOS software keyboard drops a textarea's Enter and Backspace

**Status:** Open
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A hardware key in an iOS textarea reaches `key` handlers in `pressesBegan`, before UIKit edits with it. The software keyboard does not. It calls `insertText` and `deleteBackward` directly. Those overrides edit the text and never call `keyDown`.

`TextField` does. Its `deleteBackward` calls `keyDown("Backspace")` unless `pressesBegan` already did, and `textFieldShouldReturn` does the same for Enter. A prevented key does not change the field. A textarea has no such call, so a handler for Enter or Backspace runs from a hardware keyboard and not from the software one. `preventDefault` cannot stop the software keyboard's newline or deletion.

The comment on this file says the software keyboard's Backspace and Return are keys the handlers hear.

## Where

`host/apple/Sources/ExactKit/IOS/TextAreaIOS.swift`, `TextArea.insertText`, `TextArea.deleteBackward`, and `TextArea.pressesBegan`. `TextField.deleteBackward` and `NodeView.textFieldShouldReturn` in `NodeViewIOS.swift` are the field path.

## How to reproduce

On an iPhone, focus a textarea whose `key` handler calls `preventDefault` for Enter or Backspace. Type Return or delete with the software keyboard. The handler does not run, and the text changes. The same keys on a hardware keyboard run the handler and leave the text alone.

## Direction

Give `TextArea` the `heard` guard `TextField` uses. In `insertText`, when the insertion is a newline and no composition is marked, call `keyDown("Enter")` first and skip the insertion when it returns true. In `deleteBackward`, call `keyDown("Backspace")` first and skip the deletion when it returns true. Skip the call when `pressesBegan` already delivered that key.
