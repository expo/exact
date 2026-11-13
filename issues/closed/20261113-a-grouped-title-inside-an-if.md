# A grouped title inside an if is not the row's title

**Status:** Closed
**Resolution:** title_button reads an if by its arms, as leading does: a mark or a title the button chooses with if leaves the button the row's title when every shown arm is texts and symbols and one shows a text.
**Systems:** contract
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/20261110-compiler-walks-views-and-expressions-by-hand.md

## What is wrong

`title_button` treats a row as having a title button only when the shown siblings before that button are literal `input` or symbol elements, and the button's shown children are literal `text` or symbol elements, with at least one text. An `if` before the button, or a label the button chooses with `if`, makes it return none. The button is then not the title: the later value is the first text, and the name stays in an unstretched button. The value after the button already walks `if` arms. `leading()` already treats an `if` of symbols as a symbol.

## Where

`contract/lower/src/grouped/title.rs`, `title_button`. Applied from `contract/lower/src/grouped.rs`.

## How to reproduce

A grouped row whose name is a button, with a checkmark or a title word inside `if`, and a value text after the button. The value grows mid-row and the name stays centred in the button.

## Direction

Treat a `When` as `leading()` does. Count the button as the title when every shown arm is texts and symbols and at least one arm shows a text.
