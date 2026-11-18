# A child component's `derive` is compiled again at every read, so an app's code grows silently with each one

**Status:** Closed
**Resolution:** A child's derive compiles once per component use (the component form is within 10% of its root-structured twin, enforced), and web builds report code by component; proof stages 4 and 5 are semantics/README.md's, listed in QUEUE.md.
**Systems:** contract/syntax/src/inline.rs, contract/syntax/src/inline/derives.rs, contract/syntax/src/inline/calls.rs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Component inlining substitutes a child's `derive` expression at each place it is read: "a child's `derive` is an expression substituted at each read" (`contract/syntax/src/inline.rs:80`). An action call is likewise its callee's statements expanded in place (`inline/calls.rs`, LLP 1089 D2). A root's `derive` is compiled once and read by name. So the same source costs several times as much code in a child as in the root, the factor grows with the number of reads and uses, and nothing reports it: no warning, no size by component. The web JS target has one bundle for the whole app, so every route pays for all of it before its first fetch.

## The failure

Measured at `49ae6357` with `host/web-js/build.mjs --contract`: a `derive picked = items.filter(i => i.on and i.name.length > 2)` read by `n` texts, declared in a child (`Child(items=items)`) or in the root.

| Reads | `app.js`, derive in a child | `app.js`, derive in the root |
|---|---|---|
| 1 | 50,982 B | 51,706 B |
| 10 | 56,122 B | 52,794 B |
| 40 | 73,223 B | 56,451 B |

About 570 bytes of `app.js` a read in the child against 120 in the root, the text node included. The reporting diary measured the same shape on `94b8671f`: about 530 against 55 bytes a read.

The reporting agent's real 19-section edit form, each section a component with its own derives, made an `app.js` of 9.4 MB, about 720 ms of JavaScript before the first fetch on every route. Moving all state and derives into the root and passing names down brought it to 1.46 MB, about three hours' work.

## Fix

Lowering emits executable rows per component use for child derives and
non-atomic prop or inject arguments. Reads call those bodies on the reader's
budget without storing results. Action bodies stay substituted on purpose,
so their reads keep the action's sequential state semantics. Web builds report
expanded code by component. The proof work listed under "Work still owed"
is `semantics/README.md`'s and `QUEUE.md`'s.

## Size report

Web builds report the final `app.js` size and the largest components by
their own expanded plan code and compiled instance count, including external
apps. Identical stored bodies may be shared, so the report counts them per
use.

## Implementation and acceptance

Lowering shares each used child derive and non-atomic prop or inject argument
as an executable row. Each read still evaluates it in the instance's frame,
using the reader's budget and trap handling. Rows store no result. Both the
VM and JS target share a 65,536-call work limit across nested evaluations and
bound call depth at 64; list-step accounting remains shared with the reader.
Lowering limits shared call chains to 48, leaving sixteen frames below the
runtime bound. It inlines a dependency when a new call or an existing
body's depth would exceed that margin. Distinct 65-derive chains render 65
with either a final-only read or reads of every declaration. JS call-limit
traps carry the calling `LoadDerive`'s PC, as the runner's do.
Action-body reads and action-valued derives retain substitution. Static shape
readers follow executable aliases; retained argument projections use the VM's
owner-frame cut. A compiled literal/child-derive table compares nested lists,
initial viewport policy, input capabilities, page images and retained press
and swipe dispatch.

The acceptance margin is 10% against a root-structured twin. The fixtures in
`contract/corpus/derive-form/` have 19 editable sections, 152 fields, repeated
derive reads and nested keyed rows. The component form's bundled `app.js` is
147,923 B against 137,217 B (7.80% larger); its encoded plan is 32,801 B against
30,253 B (8.42%). Tests enforce both size margins, compare runner trees and
receipts, and compare rendered JS labels at boot and after 100 seeded edits.
The edits change labels across the filter threshold, toggle inclusion, change
row keys and edit section drafts.

Source expansion remains substitution, preserving `Expand` and `instance_iff`.
The executable-call model and translation validation are described in
`semantics/README.md`; their remaining proof work is listed below.

## Work still owed

`LowerCheck` now validates root- and arm-owned rows against their substituted
expressions in every live owner instance of every reached configuration.
Lowering records the source tag of each emitted plan region; validation checks
the complete region identity before joining owners, so a folded group label is
absent without shifting the regions that follow it and a changed or reordered
roster is a reported divergence. Owner projections use checked indices and
offsets only after that roster matches. The render records each live
instance's source tag, chosen arm or key, and duplicate ordinal in its row
identity; render-owned marker rows record the item, position and match
binding. The check enumerates those records, recovers binder names from the
source region, and reconstructs plan frames and owned-slot stores without
evaluating a condition, subject, list, key or navigation property. The item,
key and arm are the render's; the row body and its substituted source are
judged at the reached configuration's current clock. A closed panel has no
recorded instances, while a prerendered panel has the instances its render
built. The reader checks the row call and every item, position and match binding
as separate VM runs. The row call covers the callee frame cut; the binding
probes cover the reader context reconstructed from the render. Caller
restoration is the VM invariant `resume_restores_environment`, and accounting
case `row-restores-reader-frame` compares it with the runner in one run. A
lowered plan cannot alter `resume`. The separate probes charge no synthetic
aggregate that the application never builds.

The focused case covers nested owners, shadowing, stateful instances,
transitive-only dependencies and recreation. Mutating its owner arm, making
its binder depth return a different enclosing value, or changing its body
produces a divergence. Folded headers, folded footers, opened and closed
panels, a prerendered panel inside a virtualized row, a reordered first-region
owner and idle clocks with nonfinite or changing keys have focused coverage.
"Executable row validation reads live owned answers without synthetic bounds"
also makes both sides read an instance-owned request's recorded answer and
currentness. Its focused mutation returns the root idle placeholder's result;
the new corpus case covers the runner path. "Executable row validation resolves
owned requests through retained frames" records each resource's plan owner;
"Executable row validation resolves owned requests through the owning arm"
retains both its region and arm. `VmRows` then resolves the read through that
arm's retained instance frame, as the runner does. Moving the request's owner
outside the frame cut or to the unselected sibling arm is a divergence. The
69 accounting cases compare `BadScope` for the cut and for all five resource
reads from the wrong arm. The restoration case also compares the returned
reader item, `2`, not only the outcome and counters. The seeded 500-case run
and the current 642-case corpus have no divergence and no instance-environment
refusal.

The checker has a deliberate limit. For a request an instance owns, its
focused mutations detect a wrong executable body, a wrong owner frame, an
owner cut out of the retained frames, and an owner moved to the wrong arm.
They do not establish arbitrary plan mutations beyond those four, operations
outside `Contract.Vm` such as `NativeProps` and linked Unicode case conversion,
or a general proof that every admitted shared load equals substitution. They
also cannot see an instance that production creates under an owned request's
placeholder and destroys after the synchronous answer within one commit. A
focused `false`-placeholder, `true`-answer case built `Temp` in the runner, but
the semantics settled the parent's request before traversing its children, so
the final `RowStore` held no `Temp` identity and both bodies received zero
contextual comparisons. Repair needs the semantics to expose that intermediate
render or record its instances. The request-settlement order inherited from
main is unchanged; this branch adds render-owned frame markers.

Further coverage is owed for a request moved to a retained ancestor whose
arguments read an inner slot: `LowerCheck.vmEnvAt` keeps the source instance's
answer while `VmRows.resourceInScope` checks only the mutated owner's retained
region and arm, whereas production evaluates the arguments in the owner frames
in `runner/src/runner/owned.rs`. It is also owed for repeated-region frames
where the nearest instance lacks the request but an enclosing instance has it:
`VmRows.resourceInScope` accepts any retained matching frame, while
`Frame::row_of` in `runner/src/vm.rs` selects the innermost frame of the region.
Finally, it is owed for successful in-scope `pending`, `failed` and `failure`
reads whose instance status differs from the root columns: `LowerCheck.vmEnvAt`
overwrites only value and currentness, and the present accounting cases reach
those reads only after wrong-arm scope has already failed. These are coverage
gaps, not reproduced results. They, the transient-instance case above and any
further undetected plan mutation are owed work rather than fixes on this
branch.

Stages 4 and 5 are `semantics/README.md`'s. Stage 3 is done: `VmRows` charges
callback and roster list work, string outputs, value nodes, string bytes and
value depth at production's points and in production's refusal order. It
transfers call and list counters and the extent memo into a row and back on
success or refusal, and unwinds active call depth. `difftest accounting` runs
69 fixed runner-against-model cases: pairs at and one past every resource and
executable-call bound, plus owned-request scope and caller restoration. Each
compares the trap, by name, and all three counters; restoration also compares
the returned value.
A roster result's size is decided from its arguments before it is built, so a
`replaceAll` whose result would be 4 GiB is refused without constructing it.
`NativeProps` and linked Unicode case conversion are outside the model and the
comparison. No production accounting defect was found.

One limit: no pair exercises `replaceAll` with a substitution template or with
an empty pattern across an astral character, because a result at the bound
exhausts the Lean interpreter. Two reviews checked that arithmetic by reading
it against `runner/src/strings.rs`, and `QUEUE.md` names the cases past the
bound that would cover it.

Stage 4 has proved the local source and row rules, but not their general
composition. `Contract.LowerSharing` states the matcher's active-depth,
instance-chain, owner-frame, region-cut, binder/free-read and cached-chain
conditions. Given valid reader and owner lowering contexts, `FrameCutCtx` and
`ScopeResolves`, `localsRespectCut_of_ctx` derives free-read value agreement.
Closed nested-owner examples discharge those structural hypotheses using
distinct local lists and item, index and match bindings, and reject a wrong
cut. No theorem yet derives them from an admitted site's real scope stack and
`ownerFrames`. `admitted_site_value_iff` proves that such a reader and owner
give the substituted expression the same source value when `Small` and
`LocalsRespectCut` hold, and `admitted_expr_correct` transports the existing
lowering theorem to the read site.

`Contract.VmRowsProof` gives successful bounded runs an inductive call tree.
`admitted_shared_load_value_iff` and
`successful_shared_load_has_inlined_run` compose the exact load rule with the
source theorem, but both still take the callee correspondence as a premise.
`LiteralGraph.returns` discharges that premise by smaller-index induction for
a finite load-only graph ending in one string literal, including transitive
loads. It does not cover arbitrary expressions emitted by lowering. The Rust
matcher test runs its exact conjunction through admitted nested-owner and
unequal arm/scope-cut cases, plus refused chain, active, frame-cut,
scope-underflow, free-read, expression and cached-margin cases. A focused
lowering test checks child derives shared between text and distinct `for` keys,
including one read only as a key; each key loads the emitted row and contains
no copied `join` body. The lowering differential uses callback and outer
values 2 and 10; replacing the callback-local read with the outer slot is a
reported value divergence.

Resource outcomes remain separate. A refusal has no value conclusion. One
finite production-ordered graph with fully inlined body `[unit, ret]` checks
that 64 shared calls return and the attempted 65th refuses `CallDepth`; a
separate example checks that both 48- and 49-call lowering-margin chains
return. An inlined body can also succeed while the shared form refuses its
call count. A stage that states a resource outcome as a theorem must first
make the walk it names total.

Stage 4 still owes a theorem for every graph emitted by the Rust sharing
transformation, with no semantic callee premise. The missing proof needs a
syntactic relation between recursive `shared_expression` interception and
`Lower.compile`, then a resource-aware trace simulation that replaces each
successful shared call with its ordinary inlined derive read. It must state
the graph's call-count and depth capacity for the source-to-shared direction;
the successful-shared-to-source direction needs no budget premise. Before
that composition can use the local source theorem, stage 4 also owes a lemma
that obtains `FrameCutCtx` and `ScopeResolves` from an admitted site's actual
scope stack and `ownerFrames`, and a generalization from `Lower.Ctx`'s empty
VM local stack and local count zero to admitted reads beneath existing
callback or `let` locals. Stage 5 must then move the shipped row-call path into
extractable production code and prove it refines `VmRows` after the other
owner-model lanes are integrated.

The strict browser comparison before and after sharing found no new failure.
`conform.mjs` over every web app with `--synthetic --build --strict`, on Linux
at `fb75c911c` (before `b2927269b`) and at `275ca744a`, failed the same steps
on both sides, apart from two. Caltrain's `app.test.contract` fails a
different test on each run, on the wasm side too, at either commit. Markdown's
JS build error did not recur when Caltrain and Markdown were run twice more on
each side. Its page fails to boot under Chrome's Linux sandbox at both commits.
Focused browser regressions cover row-field updates, metric CSS before and
after a condition changes, and executable rows in stateless and slot-owning
arms through deeper frames.
