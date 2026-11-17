# `contract --help` lists `build` without `--entry` and `verify` without six of its flags

**Status:** Closed
**Resolution:** Top-level and wrapper help list every build and verify flag; the CLI reuses their command usage text.
**Systems:** contract/cli/src/main.rs, contract/cli/src/build.rs, contract/cli/src/verify.rs, scripts/help.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The binary has two usage texts for each command: its line in the top-level `USAGE`
(`contract/cli/src/main.rs:35-49`), printed by `contract --help` and on an unknown command,
and the command's own, printed by `contract <command> --help`. Two top-level lines have fallen
behind their command:

| command | `contract --help` | `contract <command> --help` adds |
|---|---|---|
| `build` | `[-o <file.plan>] [--json] [--errors-only] [--map (requires -o)]` | `[--entry <kind> (requires -o; no --map)]` (`contract/cli/src/build.rs:5`) |
| `verify` | `[--types] [--prove <Module>]` | `[--runner-only] [--js] [--walks N] [--steps M] [--walk-seed N] [--confirm N]` (`contract/cli/src/verify.rs:10`) |

The lines for `migrate`, `symbols`, `sources`, `fmt`, `types`, `rust`, `test`, `lean`, `vocab`
and `gallery` give their command's flags in full, so a reader takes the top-level text as the
whole surface. (`forma`'s line names four of its subcommands, `check`, `eval`, `ir` and
`fmt`, and not `serve`, `gen`, `import`, `import-plan` or `export`.)

`bun exact.mjs contract --help` (`scripts/help.mjs:342-353`) repeats the short forms and also
leaves out `build`'s `--errors-only`; the module comments of `main.rs` (`main.rs:1`) and
`verify.rs` (`verify.rs:1`, which lacks `--js`) repeat them too. `--entry` is named nowhere
under `docs/`; `--runner-only`, which runs `verify` without Lean, only in
`docs/recipes/properties.md`.

## The failure

```
$ contract --help | grep -E "build|verify"
  contract build <file.contract> [-o <file.plan>] [--json] [--errors-only] [--map (requires -o)]
  contract verify <file.contract> [--types] [--prove <Module>]
$ contract build --help
usage: contract build <file.contract> [-o <file.plan>] [--json] [--errors-only] [--map (requires -o)] [--entry <kind> (requires -o; no --map)]
$ contract verify --help
usage: contract verify <file.contract> [--runner-only] [--js] [--walks N] [--steps M] [--walk-seed N] [--confirm N] [--types] [--prove <Module>]
```

## Verification

Ran `contract --help` and every command's `--help` with `target/debug/contract` and compared
the lines; `build --entry card -o card.plan` was run and is accepted. `verify`'s flags were
read, not run: it needs the semantics toolchain. `scripts/help-parity.mjs`, which the caps
check runs, compares the scripts' flags with their help rows and does not read this binary.

## Direction

Build the top-level text from the commands' own usage constants, so there is one line per
command, and bring the `contract` body in `scripts/help.mjs` to the same lines.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
