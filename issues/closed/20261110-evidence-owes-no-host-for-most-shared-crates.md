# `evidence.mjs` leaves out hosts a change owes a run: most shared crates count for none, and the first working-tree path loses its first letter

**Status:** Closed
**Resolution:** Evidence includes shared sources and their host consumers without dropping working-tree or rename paths.
**Systems:** scripts/evidence.mjs (hostsTouched, the changed-path list), LLP 1146 D2
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`bun scripts/evidence.mjs` ends with the hosts whose code the branch changed and that no
current run covers, and `--draft` prints each as `- <host>: not run against these sources`
for a PR's test plan (LLP 1146 D2: "shared layers touch every host"). Two defects make it
leave hosts out.

**The list of shared layers is short.** `hostsTouched` (`scripts/evidence.mjs:81`) names
them by hand:

```js
if (/^(runner|kernel|contract|plan|js|motion|num)\//.test(p) || …) all.forEach((h) => out.add(h));
else if (/^host\/apple\//.test(p) || …)
else if (/^host\/web(-js)?\//.test(p) || …)
else if (/^host\/linux\//.test(p)) …
```

A path under any other crate matches no branch and touches nothing. Caltrain's Apple,
Linux and web crates all link `route`, `canvas`, `color`, `logic`, `markdown`, `textflow`,
`update` and `grants` (`cargo tree -p caltrain-<host> -e normal`), and
`hostsTouched` returns `[]` for each, and for `data`, `tamber`, `bake`, `gpu`,
`host/apple-update`, `host/raster`, `host/ink`, `host/linux-update` and
`host/web-capabilities`.

**The first working-tree path is cut.** Line 24 trims git's output, and line 113 reads
`git status --porcelain` lines with `l.slice(3)`. An unstaged edit's line starts with a
space (` M runner/src/lib.rs`), the trim removes it from the first line, and the slice
then yields `unner/src/lib.rs`, which matches nothing.

## The failure

With no run recorded for Caltrain and one line appended to the files named:

| changed, unstaged | `bun scripts/evidence.mjs --app caltrain --draft` |
|---|---|
| `canvas/src/lib.rs`, `runner/src/lib.rs` | seven `- <host>: not run against these sources` lines |
| `canvas/src/lib.rs`, `route/src/lib.rs` | nothing |
| `runner/src/lib.rs` alone | nothing |

Expected: the seven lines in all three.

## Verification

Ran the three cases above, restoring the files after each; the right column is what was
printed. The second row shows the short list (`runner` in the same position owes every
host). The third shows the cut path: `runner/` is on the list and still owes nothing when
its line is the first.

## Direction

Treat a changed source path that is under no host directory and no other app as shared,
and match host directories by prefix (`host/apple`, `host/web`, `host/linux` and what rides
on them); `scripts/affected.mjs` already derives what a change reaches from Cargo's
dependency graph. Read the porcelain lines before trimming, or use
`git status --porcelain -z`.

## Related

No issue or QUEUE entry names `evidence.mjs` or `hostsTouched`. `scripts/evidence.test.mjs`
covers `hostsTouched` for `runner/`, `host/apple/`, `host/web-js/` and `host/windows/` only.
