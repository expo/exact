# `history` lists a merged branch's design edits twice; the protocol says first-parent only

**Status:** Closed
**Resolution:** `designLog` walks the log with `--first-parent`, as the protocol says, so a merged branch's design edit is listed once, in the merge, and the walk and each commit's comparison use the same parent.
**Systems:** scripts/forma-history.mjs, docs/forma-protocol.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`docs/forma-protocol.md:1144` says `history` "reads the git log of the design's files, newest first and first-parent only". `designLog` runs `git log` with no `--first-parent` (`scripts/forma-history.mjs:105` at `dd6f5dcd`), so it walks every parent, and `commitOf` then compares each listed commit with its first parent (lines 72-75). Together they show a merged branch's edit twice whenever the merge itself is listed: once in the branch's commit, and again in the merge's difference from main.

git lists a merge for a path when the path differs from every parent. That is the ordinary case of a design file edited on both sides and merged cleanly.

## The failure

```sh
git init -q -b main
printf 'forma t\n\ntheme\n  ink = #111111\n  a = #010101\n  b = #020202\n  c = #030303\n  d = #040404\n  e = #050505\n  f = #060606\n  edge = #222222\n' > app.forma
printf 'component App\n  view\n    text "a"\n' > app.contract
git add -A; git commit -qm 'A: the design'
git checkout -q -b side
sed -i '' 's/#222222/#bbbbbb/' app.forma; git commit -qam 'B: edge on a side branch'
git checkout -q main
sed -i '' 's/#111111/#aaaaaa/' app.forma; git commit -qam 'C: ink on main'
git merge -q --no-ff -m 'M: merge side' side
```

Observed, from `{"method": "history"}` to a design server on that directory (each commit's `edits`):

```
M: merge side:             edge #222222 → #bbbbbb
C: ink on main:            ink #111111 → #aaaaaa
B: edge on a side branch:  edge #222222 → #bbbbbb
A: the design:             name, ink, a, b, c, d, e, f, edge added
```

Expected, as the protocol says and as `git log --first-parent -- app.forma` lists: `M`, `C`, `A`. The edit to `edge` then appears once, with one entry to revert it from.

## Verification

Built that repository at `dd6f5dcd`, started `formaServer({appDir})` over it and posted `history` with the local token: status 200 and the four commits above. `designLog` called directly returns the same four, with the same two diffs of `edge`. Not run: the design tool's history panel.

## Direction

Pass `--first-parent` to the `git log` in `designLog`, so the walk and each commit's comparison use the same parent. If a branch's own commits are meant to be listed, compare a merge with all its parents (or leave it out) and change the protocol's sentence.

## Related

`issues/20261110-history-ignores-params-block.md` is about which edits a commit's comparison yields, not which commits are listed. No QUEUE entry mentions merges in `history`.
