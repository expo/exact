# Enter does not indent several Contract bodies

**Status:** Closed
**Resolution:** The VS Code indent pattern also opens a block after theme, params, parts, perf, sequence, a members file's `members Name`, and a test header with an `on` host list.
**Systems:** editors/vscode
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/closed/20261110-vscode-extension-knows-each-not-for.md

## What is wrong

`increaseIndentPattern` matches `component`, `shape`, `action`, a bare `test "name"`, and a short list of other openers. It does not match `theme`, `params`, `parts`, `members`, `sequence`, or `perf`, and a `test "name" on web` fails the pattern because the `test` alternative must end at the quoted name.

Those sections take an indented body. Enter leaves the next line at the parent's column. The parser then reports an empty body, or takes the line as the next declaration instead of a step inside the test.

## Where

`editors/vscode/language-configuration.json` `increaseIndentPattern`. The bodies are `required_block` and the `sequence` / `test` steps in `contract/syntax/src/parser/`.

## How to reproduce

In a `.contract` file, type `theme` or `test "boots" on web` and press Enter. The next line is not indented. A step written there is parsed as a sibling of the test.

## Direction

Match `theme`, `members`, `params`, `parts`, `sequence`, `perf`, and `test` with an optional `on` host list, the way `shape` and `action` are matched.
