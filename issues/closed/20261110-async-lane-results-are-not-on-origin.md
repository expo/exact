# The async lane's results are not on origin, and failures on main go unattributed

**Status:** Closed
**Resolution:** The first tier runs again against expo/exact (--newest --interval 3600) and pushed refs/notes/async for afde7b25e on 2026-11-13; that baseline's failures were sorted into the lane's environment (fixed in scripts/async.mjs) and real ones, filed as 2026-11-13 issues; QUEUE.md's failing-test entries remain to be fixed or deleted.
**Systems:** scripts/async.mjs, scripts/known.mjs, QUEUE.md
**Severity:** P1
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`rules/RULES.md` puts everything slower than the gate in the async lane: "Slow verification runs asynchronously, per-commit, on fleet hardware, attributed to the breaking commit." `scripts/async.mjs` is that lane. For each commit on `origin/main` it files a new failure with `issue.mjs`, naming the commit that introduced it, and pushes what it found to origin as a git note (`refs/notes/async`, tier 2 `refs/notes/async2`), which `scripts/known.mjs` reads to sort a failing test into known, new or unknown (`docs/testing.md`, "Whose failure it is").

Neither output is reaching the repository:

- `git ls-remote origin 'refs/notes/*'` prints nothing. `origin` (`github.com/expo/exact`) holds no async notes, so `known.mjs` can only answer "unknown on origin/main" (`scripts/known.mjs:109`).
- The last issues the lane filed are `issues/closed/20260923-async-*.md` to `20260925-async-cc43410e.md`. It has filed none in the six weeks since.

Meanwhile main has failures that people and agents find by tripping on them. QUEUE.md holds at least 13 entries of the form "X fails on main", dated 2026-10-02 to 2026-11-10, none naming the commit that broke it. Among them: three script tests (2026-11-10), `sheet-ids.test.mjs` (2026-11-10), an iOS bar item share test (2026-11-07), the JS symbol fixtures (2026-11-05), the touch press test (2026-11-05), two web agent tree fixtures (2026-11-05), the flowed-link Enter test (2026-11-05), the compiler's growth test (2026-11-04) and two `exact-web-js` paint tests on Linux (2026-10-24). Each lane that meets one has to establish that it is not its own, which is the cost the lane exists to remove.

GitHub's workflows do not cover the gap. `ci` runs no test. `tests` runs the Cargo and script tests for each push to main, but it reports a failure against the run, not the commit that introduced it, and QUEUE.md ("The `tests` workflow is red on Linux for reasons older than it") records about twenty Cargo tests and 25 script tests failing there.

## Verification

Ran `git ls-remote origin 'refs/notes/*'` (no output, again at `56e325b8`), `git for-each-ref refs/notes` in this checkout (no output) and listed the issues whose author is `async lane (scripts/async.mjs)`: seven, the newest dated 2026-09-25. Not checked: whether a lane is running on another machine against a different remote.

## Direction

Run the lane against `expo/exact` and confirm that its notes arrive (`git ls-remote origin 'refs/notes/async*'`). Then fix or delete each test QUEUE.md records as failing on main, so that a red result means a new failure.
