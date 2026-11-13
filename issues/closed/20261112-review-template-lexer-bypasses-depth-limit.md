# Nested templates can abort the compiler before its expression depth check

**Status:** Closed
**Resolution:** The lexer finds a template's end with a stack of its own, so a template nested thousands deep reaches the parser's `syntax-expression-depth` refusal instead of overflowing the native stack.
**Systems:** contract/syntax/src/lexer.rs, contract/syntax/src/parser/expr.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, template scanning recursively alternates between
`template_literal_end` and `template_expr_end` without a depth bound
(`contract/syntax/src/lexer.rs:601-641`). This runs while lexing the outer
template, before the parser can apply its 64-level expression limit
(`contract/syntax/src/parser/expr.rs:7-14`, `:41-51`). A sufficiently nested
template therefore aborts the compiler process with stack overflow instead of
returning `syntax-expression-depth`. An in-process consumer cannot catch this
as a normal compile refusal.

## Verification

A local synthetic Contract contains one text node. Its expression starts as
the string `"x"`; each additional level wraps it in a template interpolation.
The existing `target/debug/contract build` produced:

| Nested templates | Source bytes | Result |
| --- | --- | --- |
| 8 | 74 | Compiles |
| 64 | 354 | `syntax-expression-depth` |
| 2,000 | 10,034 | `syntax-expression-depth` |
| 20,000 | 100,034 | `syntax-expression-depth` |
| 100,000 | 500,034 | SIGABRT, `fatal runtime error: stack overflow, aborting` |

The main-thread stack was unchanged. No app, service, or network was involved.
Inputs and output are retained under
`/tmp/exact-review-20261112/template-depth/`, including `results.txt`.

## Direction and validation

Make template delimiter scanning iterative, or enforce a nesting bound before
each recursive scan and report it as a depth refusal. Retain the parser's
existing expression limit. Check ordinary nested templates, escaped backticks
and braces, and excessive nesting on a 2 MiB thread; the excessive case must
return a diagnostic without terminating its process.

## Duplicate check

Searched open and closed filesystem issues and all 275 cached GitHub issues.
Read the closed `20261009-compiler-small-stack-depth.md`: its fix bounds view
and component expansion, and its expression checks run after this scanner.
The closed `20260901-template-closes-first-brace.md` and
`20261110-template-interpolation-leading-space-refused.md`, also read in full,
concern interpolation correctness. The new shape-graph depth issue concerns a
separate type-checker walk. None covers recursion during template lexing.
