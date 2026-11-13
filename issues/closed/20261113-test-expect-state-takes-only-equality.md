# A test's `expect state` takes only `==`, so a value a drag or the clock sets cannot be asserted as a range

**Status:** Closed
**Resolution:** Scripted state assertions accept inclusive numeric bounds and typed literal membership, with observed values on failure.
**Systems:** contract/syntax/src/parser/steps.rs (`expect state`), docs/contract-grammar.md, docs/start-here.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

In a `test` block, `expect state <path>` must be followed by `==` and a literal
(`steps.rs:1165-1189`: `self.expect_punct("==")`, then a number, a string, a bool, `none`
or `[]`). A property's `expect state` already takes `>=`, `<=`, `in`, `whole` and `stays`
(`contract/syntax/src/parser/properties.rs:77-103`, `contract-grammar.md:820-831`). So
a test cannot say "the dragged offset is between 90 and 110" or "the count is at least 1". Where the exact value depends on the host, as a
drag's does (pixel rounding, a host's drag slop), the test has to name one host's value
or leave the step out.

## The failure

On Exact `fc149195`, an app's test dragged a node and wanted to assert where it ended. With
only `==` the value could not be asserted across hosts, and the flow was dropped from the
test file.

## Fix

Admit in a test's `expect state` the comparisons that properties already take (`>=`, `<=`,
`in`), with the same literals on the right, and list them in the "State" row of
`start-here.md`'s assertion table. The failure message should name the value read, as `==`
does now.
