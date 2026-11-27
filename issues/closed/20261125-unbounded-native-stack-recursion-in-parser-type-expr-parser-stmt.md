# Unbounded native-stack recursion in `Parser::type_expr`, `Parser::stmt`, `Parser::json_value`, and type-depth checks crashes the compiler

**Status:** Closed
**Resolution:** Bound nesting depth in Parser::type_expr, Parser::stmt, and Parser::json_value (appending JSON into one buffer), make type-depth walks iterative, and split platform-match helpers out of Parser::node_match.
**Systems:** contract-syntax (parser), contract-types (type resolution & depth bounds)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, `contract/syntax/src/lexer.rs:111-112, 549-553` imposes no limit on indentation or bracket nesting, and three parser paths construct arbitrarily deep recursive trees without any depth bound—crashing `contract check` / `contract build` / `contract::tests` with a native stack overflow instead of returning a diagnostic:

1. **`Parser::type_expr` (`contract/syntax/src/parser.rs:873-932`) and type-depth checks:** `option<...>` (`:904`) and retired `list<...>` (`:911`) recurse unboundedly in `type_expr`, and the iterative postfix `[]` loop (`while self.at_punct("[") && matches!(self.peek2(), TokenKind::Punct("]"))`, `:926-930`) builds an arbitrarily deep `TypeExpr::List(Box<...>)` chain on one line with zero parser stack usage. Every downstream traversal over `TypeExpr` / `Ty` recurses without a bound *before* `Shapes::bounded` (`MAX_TYPE_DEPTH = 64`, `contract/types/src/bounds.rs:15, 80-99`) runs: `Shapes::action_in` (`contract/types/src/held.rs:19-27`, called by `resolve_value`), `Shapes::resolve_unbounded` (`contract/types/src/lib.rs:335-359`), `option_in_option` (`bounds.rs:103-112`), `Shapes::depth` (`bounds.rs:21-27`), `Rescope::ty` (`contract/syntax/src/scope.rs:559-596`), `VisitSpans` (`contract/syntax/src/spans.rs:45-49`), `Display for TypeExpr` (`contract/syntax/src/lib.rs:266-284`), `fmt::ty` (`contract/syntax/src/fmt.rs:410-434`), and Rust's recursive `Drop`.
2. **`Parser::stmt` (`contract/syntax/src/parser.rs:979-1051`):** Neither `Parser::stmt` nor `Parser::block` (`:303-326`) tracks statement nesting depth (`Parser` at `:128-160` tracks `depth` for expressions and `view_depth` for views, nothing for statements). Both nested `if`/`match` blocks and flat `else if` chains (`:992`: `otherwise = vec![self.stmt()?]`, zero indentation growth) overflow the native stack in `Parser::stmt` and in downstream recursive statement walks (`scope.rs:604`, `inline/calls.rs:47, 414`, `types/src/actions.rs:81`, `types/src/posts.rs:52`, `lower/src/stmts.rs:60`, `analyze/src/suggestions/values.rs:412`, `Drop`).
3. **`Parser::json_value` (`contract/syntax/src/parser/steps.rs:500-539`):** Reachable from `test` steps (`request` at `:373, 376`, `prefer` at `:578`, `signed-in` at `:998`, `stored` at `:1004`, `answer source` at `:1078`, `answer fetch` at `:1112`). It has no depth bound on nested `[` / `{` (`:517`), and allocates a new `String::from(open)` at each recursion level (`:506`) then copies the entire inner string up via `out.push_str(&self.json_value()?)` (`:517`), causing both native stack overflow and $O(N^2)$ allocation/copying (`answer fetch`'s 64 KiB check at `:1127` runs only after `json_value` returns).

### Reachable triggers
Small inputs passed to `contract check`, `contract build`, or `contract::tests`:
- `shape S\n  x: string[][][]...` (20,000 `[]` pairs on one line) or nested `option<option<...>>`.
- An `action` with ~3,000 flat `else if` branches or nested `if`/`match` blocks.
- A `test` block containing `answer source s with [[[...]]]` or `request POST "/x" [[[...]]]` with ~3,000 nested brackets.

### Actual vs. expected
- **Actual:** Process aborts with `fatal runtime error: stack overflow` (SIGABRT/SIGSEGV), and deeply nested JSON test steps additionally do $O(N^2)$ string copying.
- **Expected:** Every excessive nesting returns a clean syntax/type refusal without aborting on a 2 MiB thread. Established by `contract/syntax/src/parser/expr.rs:7-15` (*"these bounds keep a compile inside a 2 MB thread's stack even unoptimized ... past them is a refusal, never a stack overflow"*), `contract/syntax/src/parser.rs:1140-1144` (`view_depth >= 256`), `contract/syntax/src/lexer.rs:707-710` (explicit stack for templates), and `contract/types/src/bounds.rs:15, 31-33` (`MAX_TYPE_DEPTH = 64`; `measure_shapes` uses a heap stack *"without exhausting the native one"*).

### Verification and siblings checked
Verified by reading the lexer, parser, and type-resolution passes at `9c0431fd5`. Siblings checked:
- `contract/syntax/src/parser/expr.rs:14-15, 34-58`: properly enforces `MAX_NESTING = 64` and `MAX_TREE_DEPTH = 100`.
- `contract/syntax/src/parser.rs:1140-1148` & `parser/view.rs:29, 224`: view nodes (including view `else if`) properly enforce `view_depth >= 256`.
- `contract/syntax/src/lexer.rs:707-746`: `template_end` uses an explicit heap stack (fixed in `20261112-review-template-lexer-bypasses-depth-limit.md`).
- `contract/syntax/src/lexer.rs:297-322` (`prefer extension`): parses JSON via `serde_json::Deserializer`, which enforces its default recursion limit of 128.
- `contract/syntax/src/parser/steps.rs:434-496` (`clock_until` / `until_scalar`): accepts only a single-level scalar list, not nested lists.

### Suggested fix and regression test
- In `Parser::type_expr`, enforce a depth bound (e.g. matching `MAX_TYPE_DEPTH = 64` or slightly above so `type-too-deep` still fires for moderate inputs) across `option<...>`, `list<...>`, and the postfix `[]` loop, and make `action_in`, `resolve_unbounded`, `depth`, and `option_in_option` iterative or bounded.
- Add a `stmt_depth` bound to `Parser::stmt` (covering both nested blocks and `else if` chains, matching `view_depth` or a safe smaller limit for 2 MiB debug stacks).
- Add a depth limit to `Parser::json_value` (e.g. 128, matching `serde_json`) and write into a single `&mut String` buffer instead of returning a new `String` at each level.
- Add 2 MiB-thread regression tests for `string[][][]...` (20,000 pairs), deeply nested `option<...>`, 3,000 flat `else if` statements, and 3,000 nested JSON brackets in a `test` step, asserting clean `SyntaxError`/`TypeError` refusals without abort.

### Siblings checked

contract/syntax/src/parser/expr.rs:14-15,34-58 (bounded MAX_NESTING/MAX_TREE_DEPTH); contract/syntax/src/parser.rs:1140-1148 & parser/view.rs:29,224 (view and view else-if bounded by view_depth >= 256); contract/syntax/src/lexer.rs:707-746 (iterative template_end); contract/syntax/src/lexer.rs:297-322 (prefer extension uses serde_json default depth 128); contract/syntax/src/parser/steps.rs:434-496 (clock_until is 1-level list only).
