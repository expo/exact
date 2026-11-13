# A deep acyclic shape graph aborts before the type-depth refusal

**Status:** Open
**Systems:** contract/types/src/checks.rs, contract/types/src/bounds.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, the compiler aborts on a long chain of
otherwise ordinary shape declarations. It should return `type-too-deep`,
as it does for a shorter chain exceeding the same 64-level limit.

The shape-cycle check follows each edge recursively
(`contract/types/src/checks.rs:327-367`). Acyclic edges have no depth bound.
The later shape-depth calculation also follows the graph recursively
(`contract/types/src/bounds.rs:31-58`), before the caller can reject the
computed depth. Neither pass should need a native stack frame for every
shape in an invalid input.

Verified with the newly built `target/debug/contract` on this macOS
checkout, with the normal main-thread stack. Generate a chain with:

```python
from pathlib import Path
n = 20000
text = "\n".join(
    f"shape S{i:05d}\n  next: " +
    (f"S{i + 1:05d}" if i + 1 < n else "number")
    for i in range(n)
)
Path("/tmp/shape-chain.contract").write_text(
    text + '\ncomponent App\n  view\n    text "hello"\n'
)
```

Run `target/debug/contract build /tmp/shape-chain.contract --json`.
For 20,000 and 50,000 shapes, the process terminated with SIGABRT and
`fatal runtime error: stack overflow, aborting`; it emitted no diagnostic.
The 64-shape control compiled. The 65-, 1,000- and 5,000-shape controls
returned `type-too-deep`. The fields are not recursive, and the view is
only one text node. Retained fixtures are in
`/tmp/exact-review-20261112/shape-depth/`.

Use explicit traversal state for cycle detection and depth measurement,
or refuse excessive depth before descending. Preserve the existing
64-level admission boundary and cycle diagnostic. Validate long acyclic
chains in both declaration orders, a cycle near a chain's end, a wide
shallow graph, and the accepted 64/rejected 65 boundary on a 2 MiB thread.

Checked open and closed filesystem issues and all 275 cached GitHub
issue bodies. The closed `20260901-recursive-shapes-stack-overflow.md`
fixed cyclic graphs; this graph is acyclic. The closed
`20261009-compiler-small-stack-depth.md` covers view/component nesting.
The open shared-walker issue covers view/expression traversal maintenance,
not type-graph depth. No matching GitHub report was found.
