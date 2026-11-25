# Exponential bytecode/work blowup in `fn` expansion (`lower/src/expr.rs`) and sibling `analyze::suggestions::values`

**Status:** Open
**Systems:** contract-lower (expression bytecode compiler), contract-analyze (suggestions evaluator)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, `contract/lower/src/expr.rs:430-472` expands every `Expr::Call` to a user `fn` inline while checking only call depth (`if l.fn_depth > 32` at `:435`) and never bounding total expansions or emitted bytecode size (`plan/src/asm.rs` has no size cap either). Although `contract/lower/src/lib.rs:447-452` preprocesses each `fn` body with `contract_syntax::share_calls` (`contract/syntax/src/share.rs:16-102`), `share_calls` binds a repeated call only when its argument ASTs are structurally identical and at least one occurrence is always evaluated (`share.rs:7-9, 25-28, 133-140`, tested at `:226, 232`). That collapses identical unconditional calls like `f0() + f0() + f0()`, but leaves distinct-argument calls and conditional/short-circuit/arrow calls unshared. In sibling `contract/analyze/src/suggestions/values.rs:250-275, 344-365`, `Facts::eval` does not run `share_calls` and removes `key` from `seen` on exit (`seen.remove(&key)` at `:273` and `:363`) with no memoization, step limit, or depth bound.

### Reachable triggers
1. **Lowering (`contract build` / `contract::compile`):** A 22-line valid Contract file with distinct-argument (or conditional) fanout-3 `fn` calls passes `contract_types::check` in linear time (`contract/types/src/lib.rs:1261-1280` checks bodies once against signatures; `check_function_cycles` at `contract/types/src/checks.rs:49-109` memoizes completed subgraphs via `states[index] == 2`), bypasses `share_calls`, stays within `fn_depth <= 20 <= 32` and `*locals <= 20` (locals are popped at `expr.rs:467-470`), and triggers $3^{20} \approx 3.49 \times 10^9$ inline expansions in `expr::compile`:
   ```contract
   fn f0(x: number): number = x
   fn f1(x: number): number = f0(x) + f0(x + 1) + f0(x + 2)
   ...
   fn f20(x: number): number = f19(x) + f19(x + 1) + f19(x + 2)
   component App
     view
       text "${f20(0)}"
   ```
   The same blowup occurs with conditional identical calls such as `fn f1(x: number): number = (x > 0 ? f0(x) : 0) + (x > 0 ? f0(x) : 0) + (x > 0 ? f0(x) : 0)`.
2. **Suggestions checker (`contract verify` -> `semantics/difftest/src/suggest.rs:333` -> `contract::check_suggestions_path` -> `contract_analyze::check_suggestions` -> `Facts::new` / `Facts::check`):** Even identical-argument `fn` DAGs (`fn f{n}(): number = f{p}() + f{p}() + f{p}()`) referenced by a `state` initializer, action assignment, `for` list, or checked `derive`—or a 26-derive DAG (`derive d{n} = d{n-1} + d{n-1}`)—blow up exponentially in `Facts::eval`, and a long linear `derive` chain overflows the native stack.

### Actual vs. expected
- **Actual:** `contract build` hangs and OOMs allocating bytecode for `expr::compile`; `contract verify` / `check_suggestions` hangs on `fn` or `derive` DAGs and aborts with stack overflow on long linear `derive` chains.
- **Expected:** Both passes enforce a total work/expansion budget and return a diagnostic (or fall back to `Value::Unknown` in `Facts`) rather than hanging or exhausting memory/stack. This expectation is established by `contract/lower/src/keyframes.rs:84-89, 176-187, 892-910` (`FOLD_STEPS = 4096`, `EXPANSION_DEPTH = 128`, unit test `repeated_palette_calls_have_a_total_work_bound`) and `contract/syntax/src/inline/calls.rs:40-41, 551-561` (`LIMIT = 1024` expanded statements for action calls).

### Verification and siblings checked
Verified by reading the full call path at `9c0431fd5` (note that C093's original 0-arg `f{p}() + f{p}() + f{p}()` trigger is neutralized in `lower/src/expr.rs` by `share_calls`, whereas distinct-argument and conditional DAGs bypass `share_calls` cleanly, and `suggestions/values.rs` blows up on both). Siblings checked:
- `contract/lower/src/keyframes.rs:84-89, 176-187`: properly bounded by `FOLD_STEPS` and `EXPANSION_DEPTH`.
- `contract/syntax/src/inline/calls.rs:40-41, 551-561`: properly bounded by `LIMIT = 1024`.
- `contract/analyze/src/gates.rs:79-124`, `contract/analyze/src/properties.rs:147-196`, `contract/syntax/src/inline/use_node.rs:627-649`: insert into `seen` without removing on exit (permanent visited set, no DAG blowup).
- `contract/lower/src/warnings.rs:476-498, 913-923`, `warnings/launch.rs:134-172`, `warnings/own_list.rs:18, 32-66`: cap `fn` expansion at depth `4`.
- `contract/lower/src/anatomy/platform.rs:237-286` and `contract/cli/src/lean.rs:509`: summarize or emit each `fn` once without recursive expansion.

### Suggested fix and regression test
- Track a per-expression (or per-`Lowerer`) `fn` expansion / bytecode-step budget in `contract/lower/src/expr.rs` (returning a stable error such as `lower-fn-size` once exceeded, analogous to `syntax-call-size` and `keyframes.rs`'s `FOLD_STEPS`).
- In `contract/analyze/src/suggestions/values.rs`, memoize pure `derive` evaluations per fixpoint round (and bound total `eval` steps and recursion depth, returning `Value::Unknown` when exceeded).
- Add unit/integration tests covering (a) a depth-20 distinct-argument and conditional `fn` DAG rejected promptly by `contract::compile` with a diagnostic, and (b) `fn` DAG, `derive` DAG, and deep linear `derive` chains completing promptly in `contract_analyze::check_suggestions` on a 2 MiB thread.

### Siblings checked

contract/lower/src/keyframes.rs:84-89,176-187,892-910 (bounded via FOLD_STEPS/EXPANSION_DEPTH); contract/syntax/src/inline/calls.rs:40-41,551-561 (bounded via LIMIT); contract/syntax/src/share.rs:16-102 (only shares identical strict calls); contract/analyze/src/gates.rs:79-124, contract/analyze/src/properties.rs:147-196, contract/syntax/src/inline/use_node.rs:627-649 (permanent seen sets); contract/lower/src/warnings.rs:476-498,913-923, warnings/launch.rs:134-172, warnings/own_list.rs:18,32-66 (depth capped at 4); contract/lower/src/anatomy/platform.rs:237-286 & contract/cli/src/lean.rs:509 (no inline expansion).
