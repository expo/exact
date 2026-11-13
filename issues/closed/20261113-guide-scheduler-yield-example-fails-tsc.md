# A guide's scheduler.yield example fails the TypeScript examples test

**Status:** Closed
**Resolution:** The guide's scheduler.yield example is a whole exported async function with the types it uses, so tsc checks it as a module.
**Systems:** docs/contract-for-agents.md, contract/cli/tests/it/typescript.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`contract`'s `typescript::every_typescript_example_in_the_guides_checks_against_its_contracts_types`, an `async lane:` test, fails: 1 of 14 examples fails `tsc`. It is the `ts` block in `docs/contract-for-agents.md` (line 1135 at 69668888a) that yields every 2,000 items with `await scheduler.yield()`: a fragment, so `tsc` finds no `nodes`, `comments` or `toComment`, and a top-level `await` in a file that is not a module.

Either the block gives the declarations it uses (and `export {}`), or the test is told how to skip a fragment.

## How to reproduce

```
cargo test -p contract --test it typescript:: -- --ignored every_typescript_example_in_the_guides_checks_against_its_contracts_types
```

Fails at 69668888a on the async lane's Mac and failed in the lane's run at afde7b25e.
