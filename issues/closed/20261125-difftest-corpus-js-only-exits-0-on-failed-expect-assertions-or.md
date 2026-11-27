# difftest corpus --js-only exits 0 on failed expect assertions or refused corpus programs, and async.mjs caches that run as a pass

**Status:** Closed
**Resolution:** Fail difftest corpus --js-only on failed expectations and strict compiler refusals in js::report, and run failures() before Store.record() in async.mjs so exit-0 logs with failures are never cached as passes.
**Systems:** semantics/difftest, scripts/async.mjs
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Vacuous or bypassed tests)

Verification: verified by reading.

At `9c0431fd5`, two defects combine so `difftest corpus --js-only` exits `0` on failing corpus expectations or uncompilable corpus programs and `scripts/async.mjs` caches that failing `difftest-js` run as a passing fingerprint:

1. In `run_corpus()` (`semantics/difftest/src/main.rs:168-179`), `expectations(s)` (`semantics/difftest/src/lib.rs:559-565`, which checks compilation, `expect` assertions, and `property_corpus::check`) runs on every scripted case, printing `EXPECT <name>: <e>` and appending `s.case.name` to `failed`. On the `Js::Only` early return (`semantics/difftest/src/main.rs:176-179`), the function returns `Ok(run_js(&cases, None, 32, "corpus", false, true)? && errors.is_empty())` without checking `failed.is_empty()`.
2. Inside `js::report()` (`semantics/difftest/src/js.rs:522-528, 599`), `JsVerdict::Refused(_)` (returned by `js::prepare()` at `semantics/difftest/src/js.rs:175` when compilation fails) only increments `refused` without printing `REFUSED` or failing; `js::report()` returns `Ok(diverge == 0 && errors == 0)`.
3. In `scripts/async.mjs:599-606`, `checkAt()` computes `outcome = outcomeOf(r.status, r.hung)` (returning `'pass'` for exit `0`) and calls `store.record(name, gated.fp, outcome, { sha })` *before* `failures(name, log, r.status)` extracts `EXPECT` lines from the log (ignoring `found.length > 0`).

### Triggering sequence

Run `cargo run -p contract-difftest -- corpus --js-only` (or the `difftest-js` step at `scripts/async.mjs:384-386`) when a corpus program fails an `expect` or property assertion (with Rust and JS agreeing on the wrong output) or fails to compile.

### Actual vs. expected result

- **Actual:** `difftest corpus --js-only` prints `EXPECT <name>: ...` and exits `0`. In `scripts/async.mjs`, `difftest-js` exits `0`, so line 600 records `'pass'` in the fingerprint store while lines 603–604 record `difftest-js: EXPECT <name>` in `result.failures` for that single run. On the next hourly run with unchanged `difftest-js` inputs, `decide()` (`scripts/async.mjs:583-588`) skips `difftest-js` as a cached pass. Because `difftest-js` is not in `gated()` (`scripts/async.mjs:194-198`, since `checks('', null, nothing())` includes `semanticsChecks()`), and because `retainSkippedFailures()` is also a no-op when `result.full` is `true` (`scripts/async.mjs:408, 412`), the `difftest-js` failure disappears from `result.failures` on the very next run.
- **Expected:** Per `semantics/difftest/src/main.rs:28-30` ("exits 1 when anything diverged, a corpus program was refused, a corpus `expect` failed, or the Lean backend refused a program the compiler accepts") and `scripts/async-inputs.mjs:5-7` (only a passing run records `'pass'`), `corpus --js-only` must exit `1` when `failed` is non-empty or any corpus case is `JsVerdict::Refused`, and `scripts/async.mjs` must not record `'pass'` when `failures()` finds failures in the log.

### Verification and siblings checked

Verified by reading `semantics/difftest/src/main.rs:157-193`, `semantics/difftest/src/lib.rs:559-565`, `semantics/difftest/src/js.rs:169-176, 514-600`, and `scripts/async.mjs:194-198, 384-386, 407-420, 581-606`. Additionally confirmed by a read-only `bun -e` probe that `failures('difftest-js', 'EXPECT corpus/x.contract: line 1:1\n', 0)` returns `['difftest-js: EXPECT corpus/x.contract']` while `outcomeOf(0, false)` is `'pass'` and `gated().has('difftest-js')` is `false`.

Sibling sites checked:
- `run_corpus()` without `--js-only` (`semantics/difftest/src/main.rs:180-193`): converts `failed` into `Verdict::Expectation` and passes `strict_refusals = true` to `report()` (unaffected).
- `run_explore()` (`main.rs:198-220`) and `run_random()` (`main.rs:267-294`): have no `expect` checks and intentionally skip compiler refusals (`strict_refusals = false` at lines 218 and 292).
- `verify::run()` (`semantics/difftest/src/verify.rs:498-531`): explicitly sets `passed = false` on `JsVerdict::Refused(_) | JsVerdict::Error(_)` after calling `js::report()`.
- `quick::run()` (`semantics/difftest/src/quick.rs:91-125`): has no `--js-only` mode and sets `ok = false` on `expectations(s)` errors.
- `scripts/async.mjs:599-606`: affects all four fingerprint-gated checks (`lean`, `vm-extract`, `difftest`, `difftest-js`) whenever a check exits `0` with log lines matched by `failures()`.

### Suggested fix and regression test

Pass a `strict_refusals: bool` flag into `js::report()` (printing `REFUSED <name>: <e>` and returning `false` when `strict_refusals && refused > 0`), and in `run_corpus()`'s `Js::Only` branch return `Ok(run_js(&cases, None, 32, "corpus", false, true, true)? && failed.is_empty() && errors.is_empty())`. In `scripts/async.mjs:599-606`, run `failures(name, log, r.status)` before `store.record()` and record `'fail'` whenever `outcome === 'pass' && found.length > 0`. Add unit tests covering `js::report` with `strict_refusals = true`, `corpus --js-only` on a failing `expect` and a refused corpus program, and `async.mjs` refusing to cache `'pass'` when `failures()` is non-empty.

### Siblings checked

Checked run_corpus non-js-only (main.rs:180-193 — safe), run_explore/run_random (main.rs:198-220, 267-294 — intentionally non-strict on Refused), verify.rs:498-531 (explicitly fails on JsVerdict::Refused), quick.rs:91-125 (checks expectations errors), and all four fingerprint-gated checks in scripts/async.mjs:581-606.
