# README.md describes the scaffold as it was, limits that are gone and a design corpus the repository does not hold

**Status:** Closed
**Resolution:** The README's quick start runs `bun exact.mjs test web`, describes the scaffold `exact new` writes and its generated `.exact/shells/` workspace, replaces `app.ts` whole for the todo example, no longer lists the camera as out, and says where the private corpus is cloned (items 4 and 6's links and npm sentence had already gone in a60c83e5 and earlier).
**Systems:** README.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Six statements in `README.md` at `dd6f5dcd` are false for a reader of the public
repository.

1. **Line 308**, quick start step 5: `bun exact.mjs test web tests/*.test.contract`. A new
   app has `app.test.contract` at its root and no `tests/`. zsh, macOS's shell, prints
   `no matches found: tests/*.test.contract` and runs nothing (exit 1). bash passes the
   pattern on and the verb prints `no test file matches tests/*.test.contract` (exit 2).
   `bun exact.mjs test web` is the command that works.
2. **Lines 313 to 321**: `exact new` creates "small `web/` and `apple/` host crates" and
   "It has its own Cargo workspace". The new app holds `AGENTS.md`, `CLAUDE.md`,
   `app.contract`, `app.json`, `app.test.contract`, `app.ts`, `exact.mjs`, `package.json`
   and no crate or `Cargo.toml`. Its own `AGENTS.md` says a native verb builds in a
   generated workspace the first time it runs (`game/new.mjs:886-888`).
3. **Lines 362 and 363**: "In `app.ts`, keep the generated `export const appId = …` line".
   The generated `app.ts` has no such line; only `exact new --snapback` writes one
   (`game/new.mjs:364`).
4. **Lines 702 to 704**: "TypeScript can't import npm packages' code yet … only types". A
   package's code runs. With `node_modules/tinypkg/index.js` exporting
   `shout(s) { return s.toUpperCase() + "!"; }` and `import { shout } from 'tinypkg'` in
   the new app's `app.ts`, `contract build --json --errors-only` printed `[]` and `test web`
   passed `expect text "greeting" == "HELLO FROM HELLO.!"`. `docs/start-here.md:249` says
   the same: `app.ts` imports "pure-JS npm packages".
5. **Line 710**: "Out on purpose for v1. Camera access". `rules/DEFERRED.md:932` moved the
   camera in on 2026-10-02 and names the `@exact/camera` package, which `packages/camera`
   holds.
6. **The `llp/` corpus is presented as part of the repository.** Line 59 says the
   predecessor's documents "are imported under `llp/research/`", line 604 that `llpview`
   "reads this repository's design corpus", line 716 that "`llp/current/` holds the design
   documents in play", and the repository map lists "`llp/` | The design corpus" (line
   732). The corpus is private and git ignores `llp/` (`AGENTS.md:20`, `.gitignore:70`),
   and the README never says so. Its 29 links into `llp/` then read as broken rather than
   as citations of a record kept elsewhere.

## Verification

Items 1 to 4 were run in apps `bun scripts/exact.mjs new` made outside the repository (item
1 under `zsh -c` and `bash -c`; item 4 on the web host). Item 5 was read. Item 6:
`git ls-files llp` prints nothing, and `grep -o "](llp[^)]*)" README.md` counts 29.

## Direction

Correct items 1 to 5 in place. For item 6, say once that the design record is a private
corpus cited by number, as `AGENTS.md:20` does, and take `llp/` out of the repository map
and out of lines 59, 604 and 716.

## Related

`QUEUE.md:592` ("The README's example after it says less") covers the todo screenshots and
`setup --check` in a nested worktree, none of the above.
