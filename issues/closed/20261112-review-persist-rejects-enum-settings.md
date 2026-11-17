# Persist refuses settings whose strings have a finite vocabulary

**Status:** Closed
**Resolution:** persist admits an enum, an option of one and a list of them as string settings with their declared type; a stored member a later version dropped falls back to the initializer and the journal names the enum.
**Systems:** contract/types/src/persist.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At audited commit `f6be99af`, adding an enum type to an otherwise valid
persisted string setting makes the program fail to compile:

```contract
component App
  state mode: enum<"light", "dark"> = "light" persist
  view
    text "Settings"
```

`target/debug/contract build <file> --json` exits 1 with
`type-persist-type`: it says to persist this value's parts or keep app data
in a source. Changing the type to `string` exits 0. The same refusal occurs
for `option<enum<"light", "dark">>` initialized with `some("light")` and
`enum<"light", "dark">[]` initialized with `["light"]`. All four cases
were run with the compiler built from the audited checkout; fixtures are
in `/tmp/exact-review-20261112/enum-persist/`.

The scalar predicate at `contract/types/src/persist.rs:11-13` omits
`Ty::Enum`, and both the direct and option/list cases use it at lines
29-30. These are ordinary string values with a membership constraint:
`Ty::is_string` includes them (`contract/types/src/lib.rs:128-130`), and
`docs/contract-grammar.md:1238-1244` describes their string representation.
LLP 1116 D5 admits string settings and options/lists of them. An appearance
or sort-order setting should be able to keep its declared vocabulary when
it is persisted.

Admit enums in the scalar predicate while preserving their declared type.
The runner already decodes a string and checks the restored value against
the slot's full type (`runner/src/runner/persist.rs:37-40`). Validate scalar,
optional and list enum settings across save/relaunch, including a stored
member removed by a later version: it must fall back to the initializer
and report the mismatch. Keep the existing refusals for records and nested
collections.

Searched open and closed filesystem issues and all 275 cached GitHub issue
bodies. The closed enum-state assignment and enum-mutation assignment
issues concern membership during assignment, and the closed TypeScript
summary issue concerns printing enums. GitHub #102 concerns a persistence
warning on durations; #174 concerns keeping a GPU surface's launch record.
None covers enum settings being refused by `persist`.
