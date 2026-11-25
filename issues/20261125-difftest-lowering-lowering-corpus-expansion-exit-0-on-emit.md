# difftest lowering/lowering-corpus/expansion exit 0 on emit failures and unread corpus scripts, and async.mjs ignores several difftest failure prefixes

**Status:** Open
**Systems:** semantics/difftest, scripts/async.mjs
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Vacuous or bypassed tests)

Verification: verified by reading.

At `9c0431fd5`, three related gaps in `semantics/difftest` and `scripts/async.mjs` silently swallow or drop failures from `lowering`, `lowering-corpus`, and `expansion`:

1. **`lowering::run()` never fails on `Verdict::Emit` (or `Verdict::Refused` in corpus mode).** At `semantics/difftest/src/lowering.rs:374-376`, `prepare(&case, index)` failures for `Verdict::Refused(e)` and `Verdict::Emit(e)` push `(case, None, Some(...))`. In the reporting loop (`semantics/difftest/src/lowering.rs:431-439`), `ready.is_none()` increments `skipped`, prints `SKIPPED <name>: emit: <e>` (and prints nothing for `refused`), and continues without setting `failed = true`. Line 486 returns `Ok(accounting && !failed)`. If every case fails Lean emission—or fails compilation in `lowering-corpus`—the command checks 0 cases and exits `0`.
2. **`run_lowering_corpus()` and `run_expansion()` discard corpus script parse errors.** Both `semantics/difftest/src/main.rs:326-332` (`run_lowering_corpus`) and `semantics/difftest/src/main.rs:359-385` (`run_expansion`) call `let (scripted, errors) = corpus(&dirs)?;`, print `SCRIPT {e}`, and return `lowering::run(...)` / `expansion::run(...)` without checking `&& errors.is_empty()` (unlike `run_corpus` at `main.rs:178, 192` and `quick::run` at `quick.rs:91-95`). If corpus scripts fail to parse (e.g., property sidecar errors where `property_corpus::cases` adds zero cases), both subcommands exit `0`.
3. **`scripts/async.mjs` `failures()` misses several `difftest` failure prefixes.** The `difftest` check runs 9 subcommands in one shell script (`scripts/async.mjs:379-383`), but `failures()` (`scripts/async.mjs:500-508`) only matches `DIVERGE|DIVERGE-JS|ERROR-JS|EXPECT|EMIT|SCRIPT|REFUSED`, `STALE`, `LEAN (ACCEPTS|REFUSES)`, and `error: *.lean:...`. It omits:
   - `ACCOUNTING` (`semantics/difftest/src/accounting.rs:1378`, invoked at `lowering.rs:358`),
   - `UNDECODED` (`semantics/difftest/src/lowering.rs:450-451`, `semantics/Contract/LowerCheck.lean:376`),
   - `OPCODES` (`semantics/Contract/LowerCheck.lean:475-478`, `semantics/difftest/src/lowering.rs:424`),
   - `EXPAND` (`semantics/difftest/src/expansion.rs:334`),
   - `STRUCTURE` (`semantics/difftest/src/expansion.rs:339`),
   - and `SKIPPED ...: emit:` (`semantics/difftest/src/lowering.rs:435`).

### Triggering sequence

Run `difftest lowering`, `difftest lowering-corpus`, or `difftest expansion` directly or through the `difftest` step of `scripts/async.mjs:379-383` when the Lean backend refuses a compiled program, a corpus script fails to parse, a corpus program fails compilation, or an `ACCOUNTING`/`UNDECODED`/`OPCODES`/`EXPAND`/`STRUCTURE` error is emitted.

### Actual vs. expected result

- **Actual:** `lowering` and `lowering-corpus` exit `0` when the Lean backend refuses compiled programs (`Verdict::Emit`) or when `lowering-corpus` encounters refused corpus programs; `lowering-corpus` and `expansion` exit `0` when `corpus(&dirs)` reports `SCRIPT` errors. In `scripts/async.mjs`, any `ACCOUNTING`, `UNDECODED`, `OPCODES`, `EXPAND`, or `STRUCTURE` failure occurring alongside any matched failure in the same 9-subcommand `difftest` run is silently dropped (because `found.size > 0` suppresses the fallback on line 507); when it occurs alone, it collapses to `difftest: exit 1 (see log)`, which `stoppedEarly()` (`scripts/known.mjs:50-51`) misclassifies as an incomplete run (`complete: false` at `scripts/async.mjs:611`).
- **Expected:** Per `semantics/difftest/src/main.rs:28-30`, `difftest` exits `1` when a corpus program is refused or the Lean backend refuses a program the compiler accepts; `corpus(&dirs)` parse errors fail the command as in `run_corpus` and `quick::run`; and `scripts/async.mjs:451` extracts every failure a `difftest` log names as a stable string.

### Verification and siblings checked

Rust subcommands verified by reading `semantics/difftest/src/lowering.rs:357-487`, `semantics/difftest/src/main.rs:27-30, 315-385`, `semantics/difftest/src/expansion.rs:313-369`, `semantics/difftest/src/accounting.rs:1366-1382`, and `semantics/Contract/LowerCheck.lean:373-377, 473-479`. Additionally ran a read-only `bun -e` probe against `failures()` (`scripts/async.mjs:452`) and `stoppedEarly()` (`scripts/known.mjs:50`): every one of `ACCOUNTING`, `UNDECODED`, `OPCODES`, `EXPAND`, `STRUCTURE`, and `SKIPPED ...: emit:` produced `['difftest: exit 1 (see log)']` (`stoppedEarly: true`) when alone and was completely dropped when paired with `DIVERGE other: x`.

Sibling subcommands checked:
- `run_corpus()` (`main.rs:164-193`) and `quick::run()` (`quick.rs:91-95, 280-307`): both check `errors` from `corpus()` and fail on `Verdict::Emit`.
- `expansion::run()` (`expansion.rs:327-331, 368`): already fails on `Verdict::Emit`, `ExpandError`, `Structure`, and `Diverge` (though note `Verdict::Refused` is also not failed for corpus cases in mixed corpus+random runs).
- `types::run()` (`types.rs:477-481, 553`): prints `REFUSED` and fails on `refused > 0`.
- `verify::run()` (`verify.rs:85-117`): sets `ok = false` on `SCRIPT` errors.

### Suggested fix and regression test

1. Give `lowering::run()` a `strict_refusals: bool` parameter (`true` for `lowering-corpus`, `false` for random `lowering`): print `EMIT <name>: <e>` and set `failed = true` on `Verdict::Emit`, and print `REFUSED <name>: <e>` and set `failed = true` on `Verdict::Refused` when `strict_refusals` is `true`.
2. In `run_lowering_corpus()` and `run_expansion()`, return `Ok(ok && errors.is_empty())` so `SCRIPT` errors always exit `1`.
3. Extend `scripts/async.mjs:501` (and a dedicated pattern for `OPCODES`, which has lines both with and without `:`) to extract `ACCOUNTING`, `UNDECODED`, `OPCODES`, `EXPAND`, and `STRUCTURE`. Add unit tests in `scripts/async-bisect.test.mjs` for those `failures()` prefixes and Rust tests in `semantics/difftest` asserting non-zero failure on `Verdict::Emit`, `lowering-corpus` refusals, and `SCRIPT` errors.

### Siblings checked

Checked all callers of corpus() and reporting paths in semantics/difftest/src/{main.rs,quick.rs,expansion.rs,types.rs,verify.rs}; run_corpus, quick, and verify check script errors and fail on Emit, expansion fails on Emit/ExpandError/Structure/Diverge (note: expansion also ignores Refused for corpus cases in mixed runs), and types fails on Refused.
