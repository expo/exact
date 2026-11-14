# Change-cycle analysis treats clearing a mutation as sending it

**Status:** Closed
**Resolution:** The change-cycle check follows a mutation's refreshes and `then` only from a send of it; assigning its result slot writes that slot and nothing more.
**Systems:** contract/analyze/src/gates.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At reviewed commit `f6be99af`, a change task that only clears a mutation result
can be refused as self-triggering. The checker follows the mutation's `then`
action even though assignment does not run that continuation.

The `effects` walk merges assignments, sends and refreshes into one set
(`contract/analyze/src/gates.rs:233-239`). The subsequent closure follows
`refreshes` and `then` for every mutation in that set (`:266-273`). It therefore
adds effects reachable only from a send to an action that merely assigns the
result slot.

## Reproduction

```text
component App
  state input = "first"
  mutation saved as shape string then afterSave
  action clearSaved
    saved = none
  action afterSave
    input = ""
  action edit(value: string)
    input = value
  task clear when input != "" key=input
    change(clearSaved)
  view
    text input
```

`contract::compile` refuses this with `analyze-change-cycle`: "task clear can
change its own gate or key". Clearing `saved` does not change `input` and does
not run `afterSave`; there is no cycle. This prevents the ordinary pattern of
clearing a stale mutation result after its input changes.

An external Rust probe compiled the source against the current compiler. As a
runtime control, it replaced only `change(clearSaved)` with
`after(1, clearSaved)`, booted the runner and advanced to 1 ms:

```text
clear-only change: Err(... id: "analyze-change-cycle" ...)
timer control: commits=1, input=Some(Str("first")),
               saved=Some(Option(None)), next_due=None
```

Command: `cargo run --offline --manifest-path
/tmp/exact-review-20261112/queue-origin/Cargo.toml --bin change_clear`. No host
or data request was executed. The timer is a control for the action's effects,
not a requested change to timer semantics.

## Direction and validation

Track mutation sends separately from writes to their result slots. Follow
`then` and send-driven refreshes only where execution can cause them; still
include a slot assignment itself when checking whether a trigger reads that
slot. Add a clear-only case alongside the existing genuine send/then cycle
cases in `runner/tests/it/change_tasks.rs`.

## Duplicate check

Searched open and closed filesystem issues and all 275 GitHub issues retrieved
on 2026-11-12. GitHub #177 requests the keyed change-action feature and is
closed; it does not report this false cycle rejection. No matching issue found.
