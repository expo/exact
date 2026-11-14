# A child component's `derive` is compiled again at every read, so an app's code grows silently with each one

**Status:** Open
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
expanded code by component. The proof work and strict browser comparison
listed under "Work still owed" remain open.

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

Equivalent substituted expressions now register one sharing candidate per
instance and owner. All 65 sibling aliases render on the runner and JS.
Registration includes derives reached only through other derives, so their
bodies stay shared without a direct view read. Static readers stop after
64 alias peels.
Compiled-source regressions cover both alias shapes; a plan test retains
the 64/65 boundary independently of equivalent-expression deduplication.

Keep this issue open for the general lowering-sharing correspondence.
`VmRows` models fresh callee machines, owner-frame selection, shared call
and callback-work counters, and restoration of the reader. `LowerCheck`
validates root-owned bodies against their checked substituted expressions
and detects a mutated row. Arm-owned bodies still need contextual validation
in instance environments, including their view readers. The model now carries
per-instance owned-slot stores and reselects them after a callee's frame cut,
restoring the reader's environment on return. These stores still need to be
supplied by contextual translation validation. The model still needs roster
list-step, string and extent accounting, and a
refinement proof against the runner. Its local invariants do not prove
the transformation's scope, binder or transitive-dependency conditions.

The strict browser conformance comparison before and after sharing also remains owed.
Focused browser regressions cover row-field updates, metric CSS before and
after a condition changes, and executable rows in stateless and slot-owning
arms through deeper frames.
