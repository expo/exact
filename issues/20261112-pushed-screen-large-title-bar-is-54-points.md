# On iOS 27 a pushed screen's large-title bar is 54 points tall; PushedTitleIOSTests fails 6 of 13

**Status:** Open
**Systems:** host/apple UIKit presenter (navigation bar, large titles), host/apple/tests/ExactKitTests/PushedTitleIOSTests.swift
**Severity:** P3
**Author:** Claude (found while verifying docs/agent-pitfalls.md)
**Date:** 2026-11-12

## What is wrong

`bun host/apple/build.mjs --test --ios --only PushedTitleIOSTests` on an iPhone 17
simulator running iOS 27.0 fails 6 of its 13 tests at `a8922c5f`: a pushed
screen's navigation bar is 54 points tall where the tests expect 106, so its
large title is collapsed when the screen opens. docs/agent-pitfalls.md tells app
authors the title opens collapsed, which matches what runs and not what the tests
and LLP 1115.002 expect.

## Direction

Find whether iOS 27 changed when a pushed `UINavigationItem`'s
`largeTitleDisplayMode` expands (compare a hand-built UIKit push on the same
simulator, per AGENTS.md's tell test), or whether a change on main collapsed it.
Fix the presenter, then drop the pitfall.
