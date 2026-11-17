# The bake-off README says twelve specs and a `feel/` directory; there are fifteen and no such directory

**Status:** Closed
**Resolution:** The README's table ties the specs to `pairs.json` instead of a count and names the feel probe by its path, `host/apple/feel/`.
**Systems:** bench/bakeoff/README.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`bench/bakeoff/README.md` at `dd6f5dcd` describes the directory in a table of "Piece" and "What it is". Two rows are wrong.

**Line 10:** "`specs/<id>/SPEC.md`, `data/` | twelve iPhone apps". `bench/bakeoff/pairs.json` has fifteen keys and `bench/bakeoff/specs/` fifteen spec directories. A round builds all of them: `bench/bakeoff/results/r34/round.log:1` reads "build: 15 pairs on f2cbe5de1c". The count was twelve through round r23 (`results/r23/round.log:1`: "build: 12 pairs") and has been fifteen since r24, with `feed`, `photos` and `timetable`.

**Line 15:** "`feel/` | the scroll-feel probe: real touches, a passive recorder in the app, metrics against UIKit's physics". Every other row of the table is a path inside `bench/bakeoff/`. There is no `bench/bakeoff/feel/`. The probe is `host/apple/feel/` (`probe.mjs`, `analyze.mjs`, `feel.m`, `README.md`), which `bench/bakeoff/round.mjs:88` reaches as `../../host/apple/feel/probe.mjs`. The only `feel` paths under `bench/bakeoff/` are the results (`results/<round>/feel/`) and one spec's `feel-steps.txt`.

## Verification

```sh
python3 -c "import json;print(len(json.load(open('bench/bakeoff/pairs.json'))))"   # 15
ls bench/bakeoff/specs      # fifteen directories, gen.mjs, images.swift
ls bench/bakeoff            # no feel/
ls host/apple/feel          # analyze.mjs feel.m probe.mjs README.md
```

## Direction

Say fifteen, or drop the count, and name the probe by its path, `host/apple/feel/`.

## Related

No issue or QUEUE entry covers the README.
