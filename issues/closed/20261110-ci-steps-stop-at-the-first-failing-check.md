# The `ci` workflow stops at the first failing check, so one failure hides the rest

**Status:** Closed
**Resolution:** Each of the Linux job's checks has the condition `!cancelled() && steps.deps.outcome == 'success'`, so all of them run once setup succeeded, whatever an earlier check did, and the job fails if any failed.
**Systems:** .github/workflows/ci.yml
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`rules/RULES.md` bans fail-fast: "Every check reports all failures in one run. […] A check that surfaces one defect per run turns N bugs into N x runtime." Each command in the workflow obeys it (`--keep-going`), but the job does not. At `56e325b8` the Linux job runs `fmt`, `caps`, `boot`, `workspace check` and `clippy` as consecutive steps with no `if:` condition (`.github/workflows/ci.yml:64-77`). GitHub Actions gives a step without one the condition `success()`, so it skips every step after one that fails ([status check functions](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#status-check-functions)).

Run 42846278108 on `5434f432` (2026-11-10), when the same job also ran the tests, shows it. `clippy` failed after 1.6 minutes, and the steps after it were skipped:

```
fmt             | success
caps            | success
boot            | success
workspace check | success
clippy          | failure
test            | skipped
script tests    | skipped
```

A formatting failure therefore says nothing about whether `caps`, `boot`, the workspace check and clippy pass on that commit, and a second defect behind the first costs another run to see.

The `tests` workflow does not have the fault: its script tests run after a failing Cargo test (`if: ${{ !cancelled() }}`, `.github/workflows/tests.yml`).

## Direction

Give `caps`, `boot`, `workspace check` and `clippy` the condition `tests.yml` gives its script tests, `if: ${{ !cancelled() }}`, so that every check runs and the job fails if any did.
