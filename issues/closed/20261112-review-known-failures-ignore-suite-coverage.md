# Failure attribution treats an untested suite as passing on main

**Status:** Closed
**Resolution:** `known.mjs` sorts each failure, and counts its rate, only against notes whose check of its kind ran it whole or ran a part holding its suites (read from the local log as the lane's `owners` reads them), so a partial run of another suite leaves it unknown.
**Systems:** scripts/known.mjs, async verification
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2
**Related:** `issues/20261110-async-lane-results-are-not-on-origin.md`

At `f6be99af`, `classify` in `scripts/known.mjs` selects the latest completed
check of the same kind (`cargo`, `bun` or `xctest`), ignoring its `covers`
list. It labels a local failure `new` whenever that note does not list the
failure, even if the note ran only an unrelated suite. The same selection
counts unrelated partial runs in the reported failure frequency.

The async lane deliberately runs affected suites and records their identities.
For example, a note for `build.mjs --test --ios --only Iframe`, covering only
`xctest:ios:Iframe`, cannot establish that a navigation test passed. The
documented meaning of `new` is that the lane ran that test and it passed;
when no covering note exists the answer must be `unknown`
(`docs/testing.md:55-59`). Incorrect attribution sends developers looking
for a regression in their branch that the baseline has never ruled out.

**Verification.** `/tmp/exact-review-20261112/known-coverage-probe.mjs` imports
and executes the unchanged `classify` function, using Bun 1.4.2 and a single
synthetic partial-run note. With
only `xctest:ios:Iframe` covered, it classifies
`ExactKitTests.Nav pop failed` as `new`. No GitHub notes were changed and no
test results were fabricated in the repository.

Choose the latest applicable note for each failure's suite, preserving
`unknown` when its coverage cannot be established. Count frequency only
over runs that executed that suite; distinguish retained earlier failures
from new observations. Check mixed partial histories, full runs, incomplete
runs and different suites within one test kind.

The related open issue concerns missing published notes. This bug is in
interpreting notes once they exist. No matching filesystem or GitHub issue
was found in the review's issue snapshot.
