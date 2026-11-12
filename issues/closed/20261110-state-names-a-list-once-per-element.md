# `state.failed`, `state.failures` and `state.resourceStatus` repeat a resource over a list as a key for each of its elements

**Status:** Closed
**Resolution:** State names each declared resource once and keeps a list's failure count across member reuse.
**Systems:** runner/src/runner.rs, runner/src/runner/perf.rs, runner/src/runner/resource_inspection.rs, runner/src/runner/each.rs, runner/src/agent.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Each element of a resource over a list is a member: a copy of the resource's row, its name included, appended after the plan's rows (`runner/src/runner/each.rs:136-141`, at `dd6f5dcd`). `agent::state` lists resources by `declared_resources()` and so leaves members out (`runner/src/agent.rs:824`), but three of its objects are keyed by resource name and walk every row:

- `Runner::failed_resources` (`runner/src/runner.rs:1191-1203`), written as `"failed":{…}` (`agent.rs:858-870`);
- `Runner::failure_counts` (`runner/src/runner/perf.rs:202-210`), written as `"failures":{…}` (`runner/src/perf.rs:385-405`);
- `Runner::resource_inspection`'s `"resourceStatus":{…}` (`runner/src/runner/resource_inspection.rs:98-104`).

A list with failed elements therefore writes its name once for the list and once more for each failed element, in one JSON object. A reader that parses it keeps the last duplicate, which is the last failed element's reason, while `failure(r)` in the app reads the first (`each.rs:196-198`). `resourceStatus` has a key for every member row, those of elements that left included.

## The failure

```
shape Got
  phase: string
component App
  state urls: string[] = ["a", "b", "c"]
  resource got = receive(u) for u in urls as shape Got
  view
    text got.map(g => g.phase).join(",")
```

`receive` answers later; fail all three replies with `<u> failed`, then read `state`. Observed, with `measure(true, false)` on as a development host has it:

```
"failed":{"got":"a failed","got":"a failed","got":"b failed","got":"c failed"}
"failures":{"got":{"count":1,"first":0,"last":0},"got":{"count":1,"first":0,"last":0},"got":{"count":1,"first":0,"last":0}}
```

and `resourceStatus` has four `got` keys. Expected: one `got` entry in each, `failed` giving the reason `failure(got)` reads (`a failed`), `failures` the list's count.

## Verification

A temporary test booted the app with `Runner::boot`, answered each request with a failure through `Runner::fulfill`, and printed `Runner::failed_resources()` and the three objects from `exact_runner::agent::state`: the lines above, and `failed_resources: [("got", "a failed"), ("got", "a failed"), ("got", "b failed"), ("got", "c failed")]`.

A second test moved a window of 25 elements through 162 pages with measuring on: `resourceStatus` then had 4,101 `got` keys and `state` was 1,013,902 bytes.

## Direction

Stop the three walks at `declared_resources().len()`: the list's own row already carries the first failure. For `failure_counts`, add a member's count to its list's.

## Related

No issue or QUEUE entry. The member rows that are never freed, which make `resourceStatus` grow, and the write records over a list that never retire are separate defects of the same feature.
