# Exponential AST blowup and unbounded recursion in child derive resolution (`inline/derives.rs`) for conditional reads

**Status:** Open
**Systems:** contract-syntax (derive resolution), contract-types
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, child derive resolution in `contract/syntax/src/inline/derives.rs:23-95, 129-157, 192-235, 251-319` (`resolved_derives`, `visit`, `summarize`, `Cx::place`) suffers from both exponential AST blowup on conditionally read derive DAGs and unbounded native-stack recursion on long linear derive chains.

### Reachable triggers
`resolved_derives(c)` runs on every `contract build` / `contract check` when any child component is used (`contract_types::check_all` -> `contract_syntax::expand_checked` at `contract/types/src/lib.rs:1374` -> `contract/syntax/src/inline/use_node.rs:379`, and also during action-binding checks at `contract/types/src/uses.rs:396`). Lowering's thunk sharing (`contract/lower/src/sharing.rs`) runs only *after* expansion completes.

1. **Exponential AST blowup:** A used child component with a 26-derive chain where each derive reads the previous one twice under a conditional, short-circuit (`&&`/`||`), `match`, or arrow expression:
   ```contract
   component App
     view
       Child(flag=true)
   component Child
     props
       flag: bool
     derive d0 = flag ? 1 : 0
     derive d1 = (flag ? d0 : 0) + (flag ? d0 : 0)
     ...
     derive d25 = (flag ? d24 : 0) + (flag ? d24 : 0)
     view
       text "${d25}"
   ```
   In `summarize` (`derives.rs:192-235`), `Ternary`, `Match`, `Binary(And | Or)`, and `Arrow` call `add_sometimes`, which saturates `reads[d]` to `2` while leaving `always[d] == false` whenever an arm skips `d`. Because `Cx::place` (`derives.rs:256`) only introduces `Expr::Let` when `s.reads[d] >= 2 && s.always[d] && !bound.contains(&d)`, no derive is ever bound, and line `:272` (`self.place(&self.bodies[*d], bound, true)`) recursively substitutes `d{k-1}` at both read sites, expanding $\ge 2^{25} \approx 3.35 \times 10^7$ copies of `d0` (over $10^8$ AST nodes).
2. **Unbounded recursion and AST depth violation:** Neither `visit` (`derives.rs:129-157`) nor `Cx::place` (`derives.rs:251-319`) guards recursion depth. A long linear chain (`derive d{k} = d{k-1} + 1` for ~3,000 derives) overflows the native stack in `visit` (for reverse declaration order, which `derives.rs:3` admits) and in `Cx::place` (in either order, ~2 frames per derive), and builds a substituted `Expr` tree far deeper than the parser's `MAX_TREE_DEPTH = 100` bound, crashing downstream recursive passes (`subst_expr`, `types::infer`, `lower`, `Drop`).

### Actual vs. expected
- **Actual:** `contract build` / `contract check` hangs and OOMs on a 26-derive conditional child chain, and overflows the native stack on a ~3,000-derive linear chain.
- **Expected:** `contract/syntax/src/inline/derives.rs:11-13` explicitly promises that *"Reads are counted through other derives, so a chain of derives each reading the one before twice resolves in linear size, not exponential"*, and `contract/syntax/src/parser/expr.rs:7-15` requires expressions passed to downstream recursive passes to stay within stack-safe tree depth bounds (`MAX_TREE_DEPTH = 100`).

### Verification and siblings checked
Verified by reading the exact expansion pipeline and `summarize`/`Cx::place`/`visit` logic at `9c0431fd5`. Siblings checked:
- `contract/syntax/src/inline/derives.rs:99-125` (`share_props`): passes `resolve: false`, so it never substitutes bodies recursively and cannot blow up exponentially.
- `contract/types/src/component/derives.rs:38-59, 158-166` (`Dependencies::new` / `visit`): has the same unbounded recursive DFS bug on both forward `reads` and `reverse` graphs (so a 3,000-derive linear chain in *either* declaration order can overflow a 2 MiB stack during Kosaraju's pass even before reaching `inline/derives.rs`, while the 26-derive conditional DAG passes `Dependencies::new` in $O(N)$ time and blows up in `inline/derives.rs`).
- `contract/syntax/src/inline/calls.rs:40-41, 551-561`: bounds expanded action statements with `LIMIT = 1024`.
- Root derives in `contract/lower/src/lib.rs` compile to `LoadDerive` rows rather than substituting into one another.

### Suggested fix and regression test
- Make `visit` in both `contract/syntax/src/inline/derives.rs:129-157` and `contract/types/src/component/derives.rs:158-166` iterative with an explicit heap stack.
- Enforce a total expanded AST node budget and depth bound inside `Cx::place` (refusing with a stable diagnostic once exceeded, or lifting repeated conditional child derives into shared rows/thunks instead of substituting them syntactically).
- Add regression tests on a 2 MiB thread (`std::thread::Builder::new().stack_size(2 << 20)`) for a 26-derive conditional child DAG and a 3,000-derive linear chain (both forward and reverse declared), asserting a clean diagnostic or linear-time completion without hang, OOM, or stack overflow.

### Siblings checked

contract/syntax/src/inline/derives.rs:99-125 (share_props uses resolve=false, safe); contract/types/src/component/derives.rs:38-59,158-166 (sibling unbounded recursive DFS in Kosaraju visit on reads and reverse); contract/lower/src/sharing.rs (runs after expansion); contract/syntax/src/inline/calls.rs:551-561 (bounded via LIMIT).
