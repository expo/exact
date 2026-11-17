# The bake-off report copies each grader's scratch scripts into the committed results

**Status:** Closed
**Resolution:** `report` copies of each graded spec only `grade.json`, `key.json`, `meta.json` and a grader's Markdown note (not `prompt.md`), and the 90 committed wrapper scripts, `r21/grades/podcasts-tools/` among them, are deleted.
**Systems:** bench/bakeoff/round.mjs, bench/bakeoff/results
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`, `bench/bakeoff/grade.mjs:50` starts the grader with its working directory set to `<round>/grades/<spec>`, the directory that receives `grade.json`, `key.json` and `meta.json`. A grader that writes itself a wrapper for the driver leaves it there. `report()` then copies the whole directory into `bench/bakeoff/results/<round>/grades/` with a filter that names only what to leave out (`bench/bakeoff/round.mjs:131`: `shots`, `log.jsonl`, `prompt.md`, `trace.jsonl`, `*.png`, `*.mov`). Everything else a grader wrote is copied, and was committed.

## The failure

`bench/bakeoff/results/*/grades/` holds 91 tracked files that are not one of the three records, in all fifteen rounds (`r21` to `r34` and `s1`):

| File name | Count |
|---|---|
| `d.sh` | 64 |
| `d` | 5 |
| `a.sh`, `b.sh`, `A`, `B` | 4 each |
| `A.sh`, `B.sh`, `da`, `db`, `pd.sh` | 1 each |
| `GRADING-ABORTED.md` (`r23/grades/settings/`) | 1 |

The 90 other than the note are executable shell wrappers. Each runs the driver by an absolute path under one developer's home directory, in a checkout that is not this one, and each passes a simulator UDID:

```
/Users/<home>/projects/exact2-bakeoff/bench/bakeoff/drive.mjs
```

One grader wrote outside its directory, and the report copied that too: `bench/bakeoff/results/r21/grades/podcasts-tools/d`, a sibling of `grades/podcasts/`. Nothing reads these files: `report.mjs:63` reads `grade.json` and `key.json`, and no file outside `results/` names them.

Expected: `results/<round>/grades/<spec>/` holds the grade, its key and its meta, as `bench/bakeoff/README.md:26-27` says ("`report` copies records, feel results, grades, builders' notes and the summary").

## Verification

Listed the tracked files under `bench/bakeoff/results/*/grades/` other than `grade.json`, `key.json` and `meta.json`: 91. Of these, 90 start with `#!`, contain the path above and contain `--udid`; the home directory name is the same in all 90.

Ran a copy of `round.mjs`, `report.mjs` and `pairs.json`, placed outside the repository at the same relative paths, as `round.mjs zzprobe report` with `HOME` set to a temporary directory. The round's `grades/chat/` held the three records, `d.sh`, `.tools/d.sh`, `shots/A-1.png`, `log.jsonl` and `prompt.md`, beside a sibling `grades/chat-tools/d`. The copy's `results/zzprobe/grades/` then held `chat/d.sh`, `chat/.tools/d.sh`, `chat-tools/d` and the three records.

## Direction

Make the `grades` copy keep `grade.json`, `key.json`, `meta.json` and a grader's written note such as `GRADING-ABORTED.md`. Delete the 90 committed wrappers and `r21/grades/podcasts-tools/`.

## Related

`issues/20261110-ds-store-ships-as-an-asset.md` is another stray file reaching a published tree, in the app asset copy. `issues/20261110-testing-guide-names-a-private-machine-and-login.md` covers a private machine's name in `docs/testing.md`.
