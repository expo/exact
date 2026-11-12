# docs/reference.md says a compile returns its first refusal, and keeps an overview of the repository that is no longer true

**Status:** Closed
**Resolution:** `docs/reference.md` describes `build --json` as every independent diagnostic up to 20, each with a `severity`, warnings printed on a success, and the stale overview and crate table are deleted.
**Systems:** docs/reference.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

**The diagnostics array.** `docs/reference.md:236-238` describes `build --json`: "`[]` on
success, or a stable `id`, `message`, original `file`, `line`, `col`, `end_col`, and `related`
array per refusal. Compilation returns the first refusal." The compiler collects every
independent refusal up to twenty; each entry also has a `severity` (`"error"` or
`"warning"`); and a build that succeeds with a warning prints that warning, not `[]`.
`docs/contract-for-agents.md:116-124` says all three. A tool written from the reference
reads one entry and takes a warning for a refusal.

**The overview.** Lines 108-149 are an earlier README: a one-line description, "Surfaces:
web, macOS, iOS, Linux", a "Start here" list and a crate table under "What exists", ending
"All four surfaces run the app". Rows of it are false now:

| Line | It says | The tree |
|---|---|---|
| 111, 148 | Four surfaces: web, macOS, iOS, Linux | `host/` also has `windows`, `android` and `terminal`; `AGENTS.md` builds tvOS and watchOS |
| 134 | `plan/`: "Depends on nothing" | `plan/Cargo.toml` depends on `exact-num`, `arcstr` and `triomphe` |
| 143 | `host/apple/`: `macos/` and `ios/` are presenters in SwiftPM, "sharing `swift/`" | There is no `host/apple/swift/`, and `macos/` and `ios/` hold three Swift files between them. The Swift is one package, `host/apple/Package.swift`, with `Sources/ExactKit` and 20 other source directories |
| 145 | `host/web/`: "runner + kernel in wasm over the real DOM … ~150 lines of glue" | `host/web/glue.js` is 1,500 lines, one of 29 `*glue*.js` files there (11,658 lines). The web build makes the JS target, `host/web-js`, which the table never names; wasm is the internal `--wasm` |
| 146 | `vendor/taffy/`: "Taffy 0.9.2 plus two Exact patches" | `vendor/taffy/Cargo.toml` is version 0.14.0; `EXACT-PATCHES.md` numbers its patches to 69 |

## The failure

```contract
shape G
  text: string
component App
  view
    Kid()
component Kid
  resource gs = greetings() as shape G
  view
    text gs.text
```

`contract build app.contract --json` prints an array of two entries, `type-child-resource`
at line 7 and `type-unknown-name` at line 9, each with `"severity":"error"`, and exits 1. A
file with thirty unknown attributes prints twenty entries. A file whose only fault is
`line-height=28` prints one entry, `lower-line-height-multiplier` with
`"severity":"warning"`, and exits 0.

## Verification

Ran `contract build --json` on the three inputs and counted the entries and their keys
(`col`, `end_col`, `file`, `id`, `line`, `message`, `related`, `severity`). Read
`plan/Cargo.toml` and `vendor/taffy/Cargo.toml`, listed `host/` and `host/apple/`, counted
the `## Patch` headings of `EXACT-PATCHES.md` (66, the last numbered 69) and the glue files'
lines with `wc -l`.

## Direction

Replace "Compilation returns the first refusal" with the agent guide's sentences and add
`severity` to the field list. Delete lines 108-149 or rewrite the table from `Cargo.toml`'s
members; LLP 1000 is the current map.

## Related

`issues/20261110-reference-misstates-the-turn-budget.md` covers lines 1048-1051 of the same
document and nothing here. Lines 2931-2939, which say `each` for `for`, belong to the issue
for the `reachend` refusal whose text they copy.
