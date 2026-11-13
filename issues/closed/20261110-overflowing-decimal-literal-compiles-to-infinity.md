# A decimal literal too large for a double compiles to Infinity, which the runtime then refuses

**Status:** Closed
**Resolution:** The lexer refuses every number literal that overflows a double as `syntax-bad-number`, with or without an exponent, at the literal.
**Systems:** contract/syntax lexer (number literals), contract/lower (`Expr::Number`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Grok 4.7's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

The lexer refuses a number literal that overflows only when it has an exponent:
`.filter(|n: &f64| n.is_finite() || !exponent)` (`contract/syntax/src/lexer.rs:328`). Rust's
`f64` parse returns `inf` for an out-of-range decimal rather than an error, so a literal with
no exponent past the largest double (about 1.8e308, a 309-digit integer) lexes as
`Number(inf)`. `min=-1e999` is `syntax-bad-number`
(`contract/syntax/tests/it/parse.rs:674-676`), and a test step's 400-digit number is refused
(`contract/cli/tests/it/tests_decl.rs:891`), but `derive enormous = 999…9` is accepted
(`contract/syntax/tests/it/fmt.rs:123` relies on it). Lowering emits the value with no check
(`contract/lower/src/expr.rs:128-130`).

Every host then refuses the non-finite value at run time, not at build. A slot initialised
from such a literal fails its type check when it is made (`runner/src/instance/region.rs:436`,
reported as `NonFiniteValue`, `runner/src/runner/errors.rs:145-151`); on the web JS target
`sig` throws (`host/web-js/rt.js:88-89`, `conforms` requiring a finite number at
`host/web-js/shape.js:10`). A program the compiler accepted does not start.

## To see it

Confirmed by reading code, and by checking with `rustc` that `"9".repeat(400).parse::<f64>()`
is `Ok(inf)`; nothing was compiled or run in the checkout. Compile a component with `state n = ` followed by 400 `9`s
and run it on any host: the build succeeds and the slot's creation fails.

## What a fix is

Refuse a non-finite literal as `syntax-bad-number` with or without an exponent, and change
`fmt.rs`'s `raw_literals_survive` to use a large literal that is still finite. Test that
the 400-digit decimal is refused as `-1e999` is.
