# A closed stage's gate, its fixtures and a one-time migration are still in the tree

**Status:** Closed
**Resolution:** The stage 11 gate and its test, `apps/scale-gate`, the two scale fixture packages (with their `package.json` and `bun.lock` entries) and the 1116.015 migration are deleted; an app still on the old spelling is refused with the edit named (`syntax-view-keyword`).
**Systems:** scripts/stage11-gate.mjs, apps/scale-gate, packages/scale-fixture-cards, packages/scale-fixture-notes, scripts/migrate-1116-015.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Two scripts whose one job is recorded as done are still tracked, with no caller (at
`dd6f5dcd`). `rules/RULES.md`: "Delete; don't deprecate. No compat shims, no migration
paths".

**`scripts/stage11-gate.mjs`** (602 lines) and `stage11-gate.test.mjs` (73). LLP 1121
records "Stage 11's gate: met (2026-10-20)". No script, document, workflow or
`package.json` names the script; the one mention is a comment at
`apps/scale-gate/app.contract:1`. Its test is not on the async lane's list (`glueTests`,
`scripts/async.mjs:119`). The script is the only user of the fixture app `apps/scale-gate/`
(six files) and of the workspace packages `packages/scale-fixture-cards` and
`packages/scale-fixture-notes` (three files each, named in `package.json:20`, `:42` and
`bun.lock`). Its header points a reader at `~/x2theme/stage11-brief.md`, and its default
`--out` is `~/x2theme/stage11-gate-run.txt`: paths outside the repository. Its one check of
its own, `citations` (every scale cell names a source in `docs/scale-measured.md`),
repeats what the kernel's build already refuses (`kernel/build/scale.rs:187-214`).

**`scripts/migrate-1116-015.mjs`** (349 lines). Its header says it is "run once over a
checkout … then deleted after the open branches have taken it". LLP 1116.015 records that
it rewrote the sources on 2026-10-13 and "stays until the open branches and the apps
outside the repo have run it". No tracked file names it. This repository's history begins
on 2026-11-01, after the rewrite, so no branch of it has the old spelling. Run on main
without `--root` it would rewrite the old spellings main quotes on purpose, as its header
warns: `bun scripts/migrate-1116-015.mjs --dry` ends "would change 276 lines in 81 files",
among them `docs/contract-grammar.md`, `contract/corpus/rejects.txt` and `game/new.mjs`.

## Verification

For each name, `git grep -n -I <name>` over the tracked files, excluding the file itself:

- `stage11-gate`: `apps/scale-gate/app.contract:1` only.
- `scale-gate`, `scale-fixture`, outside the gate, the app and the packages:
  `package.json:20`, `package.json:42`, `bun.lock`, and a comment at
  `scripts/deploy-paths.test.mjs:15`.
- `migrate-1116-015`: nothing.

`bun test scripts/stage11-gate.test.mjs` passes when run by hand (5 tests). The migration
was run with `--dry` only, which writes nothing.

Not established: whether an app outside the repository still has the old spelling. The
compiler refuses it and names the edit (`syntax-view-keyword`).

## Direction

Delete the stage 11 gate, its test, `apps/scale-gate` and the two fixture packages, with
their `package.json` and `bun.lock` entries. Delete the migration once the owner confirms
no outside app still needs it.

## Related

No issue or QUEUE entry asks for these removals. The stage 12 gate is not listed: the lane
runs its tests and QUEUE has open follow-ups that name it.
