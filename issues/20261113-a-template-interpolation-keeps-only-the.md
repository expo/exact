# A template interpolation keeps only the first line of its expression

**Status:** Open
**Systems:** contract syntax parser
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A `${…}` fragment is re-lexed and accepted when the next token is a newline or the end of the fragment. The lexer emits a newline at the end of every non-empty line outside brackets. An expression that breaks after a complete prefix, then continues, parses as only that prefix. Nothing is refused.

`${count\n+ extra}` compiles as `${count}`. A number or string prefix still type-checks, so the template shows the wrong text. Parentheses still parse, because bracket depth hides newlines. Anywhere else, a newline outside brackets ends an expression and the continuation is an error.

## Where

`contract/syntax/src/parser/expr.rs`, the template arm: after `sub.expr()`, the peek accepts `TokenKind::Newline` or `TokenKind::Eof`.

## How to reproduce

```
text `total ${count
+ extra}`
```

The plan's template expression is `count`. `extra` is not part of it, and the build succeeds.

## Direction

After the inner expression, skip only a newline that ends the fragment and require end of input. If any token remains, refuse with `syntax-template-expr` at that token.
