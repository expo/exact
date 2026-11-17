# `contract forma` lists `gen ts|rust` and `bun exact.mjs forma --help` lists `gen ts|rust|swift`; the command takes `kotlin` too

**Status:** Closed
**Resolution:** Forma's compiler and app help list all four binding languages: TypeScript, Rust, Swift and Kotlin.
**Systems:** contract/cli/src/forma.rs, scripts/help.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`. `contract forma gen` writes a design's tokens as
bindings for four languages. Two of the three usage texts that describe it are short:

| Where | It lists |
|---|---|
| `contract forma gen` with no language (`contract/cli/src/forma_gen.rs:40`) | `ts\|rust\|swift\|kotlin` |
| `contract forma` (`contract/cli/src/forma.rs:16`) | `ts\|rust` |
| `bun exact.mjs forma --help` (`scripts/help.mjs:386`) | `ts\|rust\|swift` |

`docs/contract-for-agents.md:4960` gives all four. An author who reads either help does not
learn that `gen kotlin` writes `FormaTokens.kt`, and one who reads the compiler's does not
learn of `gen swift`, which `docs/reference.md:2583` tells them to run.

## The failure

```
$ contract forma | grep gen
  contract forma gen ts|rust <app dir | app.contract | file.forma> [-o <dir>]   (bindings, in the app's .exact/forma/)
$ contract forma gen
usage: contract forma gen ts|rust|swift|kotlin <app dir | app.contract | file.forma> [-o <dir>]
$ contract forma gen kotlin app -o out
out/FormaTokens.kt
$ contract forma gen swift app -o out
out/forma.swift
```

Expected: each usage text lists the four languages the command takes.

## Verification

Ran `contract forma`, `contract forma gen` and `contract forma gen <language>` for `ts`,
`rust`, `swift` and `kotlin` on a directory holding this `app.forma` beside a one-line
`app.contract`:

```
forma sample

theme
  surface.card = light-dark(#f1f3f5, #16181c)
```

Each wrote its file (`forma.ts` and `forma.css`, `forma.rs`, `forma.swift`,
`FormaTokens.kt`) and exited 0; `gen java` printed the four-language usage. Ran
`bun exact.mjs forma --help` in an app `exact new` made: it prints `gen ts|rust|swift`.

## Direction

Make both lines read `gen ts|rust|swift|kotlin`, as `forma_gen.rs` does.

## Related

`issues/20261110-top-level-help-omits-flags.md` is the same kind of gap in `contract --help`
and in `bun exact.mjs contract --help`; it notes that the `forma` line there names no `gen`
at all. It does not cover `contract forma`'s own usage or `bun exact.mjs forma --help`.
