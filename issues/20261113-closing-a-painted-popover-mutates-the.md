# Closing a painted popover mutates the open set while it is enumerated

**Status:** Open
**Systems:** host/apple
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`drop` iterates `agentOpen` to close nested popovers, then removes its own key. The same dictionary is enumerated by the menu sync, by `children`, and by `agentShow`, and those loops call `drop`. Swift traps when a dictionary is mutated during enumeration. `lightDismiss` copies the keys first and does not trap.

A menu-row action calls `agentShow` with no prior light dismiss. A nested auto popover, a destroyed open popover, or a children op that drops one takes the same path.

## Where

`host/apple/Sources/ExactKit/IOS/MenusIOS.swift`, `drop`, the `agentOpen` loop in the sync, `children`, and `agentShow`.

## How to reproduce

Open a painted popover that has another painted popover nested in it, and dismiss the outer one. The process traps in `drop`.

## Direction

Snapshot `Array(agentOpen)` before any `drop`, and drop descendants from that copy.
