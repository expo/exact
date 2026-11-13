# A macOS field maxlength follows the stale value prop

**Status:** Closed
**Resolution:** TextInputFormatter decides an edit from the editor's original string, as TextInputLimit.allows does: it accepts a proposal within maxlength or no longer than that string, whatever the value prop's length.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`TextInputLimit.allows` rejects an edit that grows the live editor past `maxlength`, and accepts one that does not grow it. macOS fields do not use that. `TextInputFormatter.isPartialStringValid` allows a proposed string that is longer than `maxlength` when it is no longer than `props["value"]`.

An authored value may be longer than `maxlength`. After the person deletes part of it, the editor is shorter than the prop. The formatter still allows keystrokes back up to the prop's length. iOS fields and the macOS text area call `allows` with the editor string, so the same `maxlength` stops those edits.

## Where

`host/apple/Sources/ExactKit/TextInputLimit.swift`, `allows` and `TextInputFormatter.isPartialStringValid`.

## How to reproduce

On macOS, focus a text field whose `value` is longer than `maxlength`. Delete one character, then type two. The second character is accepted. The same steps in a macOS textarea, or in an iOS field, stop at `maxlength`.

## Direction

Decide the edit from the editor's current string, as `allows` does: accept the proposal when its length is within `maxlength` or no longer than that string.
