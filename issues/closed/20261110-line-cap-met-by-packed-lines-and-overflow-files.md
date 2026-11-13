# Files at the 1,500-line cap stay under it by packing lines, and code is placed where there is room

**Status:** Closed
**Resolution:** Each of the four files is split by concern and written one statement to a line, with the code the cap had misplaced moved to its owner: host/web-js/rt.js into seven modules (3f0bb666 and this commit), scripts/agent.mjs into the driver's modules with `--reader` as `open`'s option (ed29bfd7, dda177c5, e4e2f9fa), and js/src/prelude.js into a script per concern, which takes binary.js's image options and answer timers (9f478841, d2c9aa41, d013c494, 88d0fbe0).
**Systems:** host/web-js/rt.js, scripts/agent.mjs, js/src/prelude.js, js/src/binary.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`scripts/caps.mjs` counts newlines, so it cannot see how much a line holds. JavaScript files
meet the cap by packing statements together. "Reprinted" below is the count of code lines
at `dd6f5dcd` after Bun's printer (`bun build --no-bundle`) puts one statement on a line
and drops the comments.

| File | Lines | Over 200 chars | Longest | Code lines, reprinted |
| --- | ---: | ---: | ---: | ---: |
| `host/web-js/rt.js` | 1,485 | 145 | 1,141 | 1,325 to 3,073 |
| `scripts/agent.mjs` | 1,486 | 139 | 6,756 | 1,250 to 1,880 |
| `js/src/prelude.js` | 1,500 | 41 | 713 | 1,112 to 1,807 |

The cap also decides where code lives:

- `js/src/binary.js:5-7`, a file named for bytes, holds "an image call's options and an answer's
  timers (here, beside the prelude, under its line cap)".
- `scripts/agent-launch.mjs:469-471` passes `--reader` through `process.env.EXACT_ANDROID_READER`
  because "the driver's own file is at its line cap".

## The failure

`issues/closed/20260927-files-at-line-cap.md` reported this for seven files and was closed by
splitting 22. Fifteen of the 22 were within 50 lines of the cap again at `dd6f5dcd`, including
`scripts/agent.mjs`, which grew from 1,266 to 1,486 lines.

## Verification

Counted the figures above over the file set `caps.mjs` scans and reprinted the JavaScript
with `bun build --no-bundle`.

## Direction

Split `host/web-js/rt.js`, `scripts/agent.mjs`, `js/src/prelude.js` and `js/src/binary.js` by
concern and write one statement to a line. Put image options and answer timers with the code
that owns them, and the reader option with the driver's other launch options.

## Related

`issues/closed/20260927-files-at-line-cap.md` (regressed, above).
`issues/closed/20261025-a-large-root-component-cannot-meet-the-line-cap.md` is about an app's
root component, not the host's files.
