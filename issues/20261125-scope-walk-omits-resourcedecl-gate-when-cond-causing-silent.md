# `scope::walk` omits `ResourceDecl.gate` (`when cond`), causing silent cross-file call misbinding and scope-isolation bypass

**Status:** Open
**Systems:** contract-syntax, contract-cli (multi-file scoping & symbol graph)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Dropped plumbing)

Verification: verified by reading.

At `9c0431fd5`, `Rewriter::component` in `contract/syntax/src/scope.rs:446-470` walks a `ResourceDecl`'s `source`, `each`, `args`, `shape`, and `placeholder`, but completely omits `res.gate` (`when cond`). Directly below at `contract/syntax/src/scope.rs:480-497`, both `Task.gate` (`t.gate`) and `Sequence.gate` (`q.gate`) are walked. Because `scope::rescope`, `scope::bindings`, `scope::calls`, `scope::sources`, and `scope::type_names` (`contract/syntax/src/scope.rs:199-232`) all delegate to `scope::walk`, every multi-file scoping and symbol-collection query misses expressions inside `resource ... when cond`.

### Trigger
In a multi-file build (`contract build app.contract`, reaching `scope::rescope` at `contract/cli/src/sources.rs:1047`):
1. **Silent cross-file call misbinding:** `panel.contract` declares `fn ready(x: string): bool = x != ""` and `component Panel` with `resource items = fetchItems() when ready("ok") as shape Item[]`. `app.contract` imports `use Panel from "./panel.contract"`, declares its own `fn ready(x: string): bool = false`, and renders `Panel()`.
2. **Broken `use ... as alias`:** `panel.contract` imports `use check as ok from "./util.contract"` and writes `resource items = fetchItems() when ok("a") as shape Item[]`.
3. **Scope-isolation bypass (LLP 1091):** `panel.contract` calls an unimported top-level `fn` or shape declared in `app.contract` inside `res.gate`.
4. **Incomplete name collection:** A file calls a compiler/roster function (e.g. `length(...)`) or binds a local (e.g. arrow/match binder) only inside `res.gate`, which `scope::calls` (`contract/cli/src/sources.rs:1140`, `contract/cli/src/snapshot/entry.rs:478, 492, 504`), `scope::bindings` (`contract/cli/src/sources/members.rs:274-275`), and `scope::type_names` (`contract/cli/src/snapshot/entry.rs:491`) fail to see.

### Actual vs. Expected
- **Actual:** In case (1), `Loader` renames `panel.contract`'s `fn ready` to `ready__panel`, but `rescope` leaves `res.gate` unrewritten as `ready("ok")`; after units merge, compilation succeeds with zero errors and silently invokes `app.contract`'s `ready` instead of `panel.contract`'s. In case (2), the alias is never expanded and fails with an unknown-function error. In case (3), `scope::walk` never records `Missing`, so no `contract-use-missing` diagnostic is emitted and `panel.contract` silently reaches across files to `app.contract`'s declaration. In sibling `contract/cli/src/symbols.rs:791-811`, `r.gate` (and `r.each`) are likewise omitted from the symbol reference graph.
- **Expected:** Per LLP 1091 per-file scope isolation and parity with `t.gate`/`q.gate` (`contract/syntax/src/scope.rs:480-497`) and `ResourceDecl` span traversal (`contract/syntax/src/spans.rs:164`), `scope::walk` must visit `res.gate` in the component's scope, and `symbols.rs` must record references in `r.gate` and `r.each`.

### Verification & Siblings Checked
Verified by reading `contract/syntax/src/scope.rs:199-232, 446-497`, `contract/cli/src/sources.rs:1047, 1140`, `contract/cli/src/sources/members.rs:274-275`, `contract/cli/src/snapshot/entry.rs:478-504`, and `contract/cli/src/symbols.rs:791-845` at `9c0431fd5`. Siblings checked:
- `contract/cli/src/symbols.rs:791-811`: **same defect** — walks `r.source`, `r.args`, `r.placeholder`, and `r.shape`, omitting `r.gate` and `r.each` (and also omitting `m.refreshes` and `c.sequences`).
- `contract/syntax/src/scope.rs:480-497`: `tasks` visits `t.timer.0`, `t.gate`, and `t.key` (`t.options` only allows literal keywords and numbers per `contract/lower/src/timers.rs:83-118`, so skipping `t.options` is benign); `sequences` visits `q.gate`, `q.key`, `q.tracks`, and `q.options`.
- `contract/syntax/src/spans.rs:164`, `contract/syntax/src/inline.rs:329` (`free_calls`), `contract/syntax/src/inline/derives.rs:439`, `contract/syntax/src/inline/use_node.rs:406`, `contract/analyze/src/gates.rs:182`, `contract/cli/src/fence.rs:390`, `contract/cli/src/component_costs.rs:64`, and `contract/cli/src/lean.rs:583`: all properly visit `ResourceDecl.gate`.

### Suggested Fix & Regression Test
In `contract/syntax/src/scope.rs:446-470`, visit `res.gate` in the component's scope (outside `inner` since `when` and `for` are mutually exclusive per `contract/syntax/src/parser/decls.rs:146-150`):
```rust
if let Some(gate) = &mut res.gate {
    self.expr(gate)?;
}
```
In `contract/cli/src/symbols.rs:791-811`, also walk `r.gate` and `r.each` (binding `each.item` as a local for `each.key` and `r.args`).

Add multi-file tests in `contract/cli/tests/it/scope_review.rs` (or `scope` integration tests) covering (1) colliding `fn` in `res.gate`, (2) `use ... as alias` in `res.gate`, and (3) unimported `fn` in `res.gate` producing `contract-use-missing`, plus a `symbols` test asserting references inside `resource ... when` are recorded.

### Siblings checked

Checked contract/cli/src/symbols.rs:791-811 (same defect: omits r.gate and r.each, plus m.refreshes and c.sequences), scope.rs tasks/sequences (both walk .gate; Task.options only allows literal keywords/numbers per timers.rs:83-118), and all other AST walkers (spans.rs, inline.rs::free_calls, inline/derives.rs, inline/use_node.rs, analyze/gates.rs, fence.rs, component_costs.rs, lean.rs), which all properly walk r.gate.
