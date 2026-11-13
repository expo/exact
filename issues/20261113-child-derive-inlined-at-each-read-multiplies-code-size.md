# A child component's `derive` is compiled again at every read, so an app's code grows silently with each one

**Status:** Open
**Systems:** contract/syntax/src/inline.rs, contract/syntax/src/inline/derives.rs, contract/syntax/src/inline/calls.rs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Component inlining substitutes a child's `derive` expression at each place it is read: "a child's `derive` is an expression substituted at each read" (`contract/syntax/src/inline.rs:80`). An action call is likewise its callee's statements expanded in place (`inline/calls.rs`, LLP 1089 D2). A root's `derive` is compiled once and read by name. So the same source costs several times as much code in a child as in the root, the factor grows with the number of reads and uses, and nothing reports it: no warning, no size by component. The web JS target has one bundle for the whole app, so every route pays for all of it before its first fetch.

## The failure

Measured at `49ae6357` with `host/web-js/build.mjs --contract`: a `derive picked = items.filter(i => i.on and i.name.length > 2)` read by `n` texts, declared in a child (`Child(items=items)`) or in the root.

| Reads | `app.js`, derive in a child | `app.js`, derive in the root |
|---|---|---|
| 1 | 50,982 B | 51,706 B |
| 10 | 56,122 B | 52,794 B |
| 40 | 73,223 B | 56,451 B |

About 570 bytes of `app.js` a read in the child against 120 in the root, the text node included. The reporting diary measured the same shape on `94b8671f`: about 530 against 55 bytes a read.

The reporting agent's real 19-section edit form, each section a component with its own derives, made an `app.js` of 9.4 MB, about 720 ms of JavaScript before the first fetch on every route. Moving all state and derives into the root and passing names down brought it to 1.46 MB, about three hours' work.

## Fix

The reporter asked for any of: a warning when one derive is inlined many times, a size report by component from the build, or not inlining. The durable fix is to lower a child's derive once per instance, as a lifted slot or a function of the instance's props, as `state` already is lifted (`Expanded::owners`), and to call a shared action body rather than copy it. Until then, report each component's share of the plan and the bundle from the web build, and say in `docs/contract-for-agents.md`'s components section that a derive read in many places belongs in the root.
