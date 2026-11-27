# fullCoverage() is permanently false on macOS because app:shelf:windows always exits 77, forcing hourly full runs even when idle

**Status:** Closed
**Resolution:** Decouple daily full-sweep scheduling (hostFullCoverage/shouldRunFull/nextFullState) from cross-platform fullCoverage so structurally inapplicable other-OS or optional vm-extract exit-77 steps do not force hourly full runs.
**Systems:** scripts/async.mjs, scripts/affected.mjs
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Fail-open defaults)

Verification: verified by running.

At `9c0431fd5`, `fullCoverage()` (`scripts/async.mjs:618`) returns `result.full && !Object.values(result.checks).some(c => c.status === 77)`. On macOS (`process.platform === 'darwin'`), every full Tier 1 run (`scripts/async.mjs:120-125, 311`) includes two Windows app steps emitted for `apps/shelf`:

- `Cargo.toml:2` lists `apps/shelf/windows` as a workspace member, `apps/shelf/app.test.contract` exists, and `APP_HOSTS` (`scripts/affected.mjs:88`) maps `windows` to `['windows']`, so `suites(repo)` (`scripts/affected.mjs:138-148`) emits `app:shelf:windows`.
- `extraChecks()` (`scripts/async.mjs:167-179`) expands that suite into `build-app:shelf:windows` and `app:shelf:windows:0`, both tagged with `{ platform: 'win32' }`.
- `run()` (`scripts/async.mjs:517-522`) checks `if (platform && platform !== process.platform)` and immediately returns `{ status: 77, hung: false, notRun: 'needs a Windows worker' }` without spawning a child.

### Triggering sequence

Run the Tier 1 hourly lane (`bun scripts/async.mjs`) on macOS (`process.platform === 'darwin'`, the Tier 1 lane's host per `scripts/async.mjs:110`).

### Actual vs. expected result

- **Actual:** Every full run on macOS records `status: 77` for `build-app:shelf:windows` and `app:shelf:windows:0`, making `fullCoverage(result)` unconditionally `false`. Consequently, `once()` (`scripts/async.mjs:728-729`) never writes `state.fullAt` or `state.fullSha`. On every subsequent hourly tick, `!(Date.now() - Date.parse(state.fullAt ?? 0) < FULL_EVERY_MS)` evaluates to `true` (`scripts/async.mjs:712`), so:
  1. `if (!pending.length && !full) return;` (`scripts/async.mjs:713`) never triggers—the lane re-runs the entire workspace suite every hour on the same tip even when zero commits have landed.
  2. `affected.mjs` selection is permanently bypassed whenever commits arrive.
  3. `file()` (`scripts/async.mjs:625`) never emits the daily-full-run attribution notice.
- **Expected:** Per `scripts/async.mjs:3-6, 115-116, 515-516, 712-713`, Tier 1 runs `affected.mjs`-selected suites between commits, idles when no commits are pending, and performs a full sweep at most once per 24 hours (`FULL_EVERY_MS`). Commit `77dd548d8` intentionally avoided claiming full cross-platform coverage when Windows steps exit `77` (`scripts/async.mjs:617`, `scripts/async-bisect.test.mjs:86-96`), but gating `state.fullAt` scheduling on `fullCoverage(result)` turns an unrun remote-platform step into an infinite hourly full-run loop on macOS.

### Verification and siblings checked

Verified by running a read-only Bun probe at `9c0431fd5` on `darwin` against `laneSelection([], { full: true, root: process.cwd() })`, `checks()`, `run()`, and `fullCoverage()`: `checks()` emitted `build-app:shelf:windows` and `app:shelf:windows:0` with `{ platform: 'win32' }`, `run()` returned `{ status: 77, hung: false, notRun: 'needs a Windows worker' }` for both, and `fullCoverage()` returned `false`.

Sibling checks inspected:
- `vm-extract` (`scripts/async.mjs:378`, `semantics/vm-extract/check.sh:16-22`): also exits `77` (`UNAVAILABLE`) when Charon or Aeneas is absent (and `store.record` at `scripts/async-inputs.mjs:251` ignores `'unavailable'`), causing the exact same permanent `fullCoverage(result) === false` loop on any macOS host without those tools.
- `apps/bench/windows` (`Cargo.toml:2`): has a Windows crate but no `apps/bench/*.test.contract`, so it does not currently emit an `app:bench:windows` suite (adding any `.test.contract` there would hit the same path).
- `tierTwoChecks()` (`scripts/async.mjs:79-88, 312`): runs on Tier 1 via Zig cross-compilation without `{ platform }`, so it does not exit `77`.

### Suggested fix and regression test

Decouple host-local daily-sweep scheduling (`fullAt`) from cross-platform coverage reporting (`fullCoverage`), or ignore steps that are structurally inapplicable to the current OS (`opts.platform !== process.platform`, recorded via `notRun`) and/or host-unavailable optional checks when updating the daily cadence timestamp (`fullAt`), while keeping honest reporting of unrun steps in `result.checks` and `file()`. Add a test in `scripts/async-bisect.test.mjs` verifying that a macOS full run over `laneSelection([], { full: true })` advances the scheduling timestamp so an idle subsequent tick (`pending.length === 0`) returns early and a commit tick uses `affected.mjs` selection rather than forcing `full = true`.

### Siblings checked

Checked vm-extract (scripts/async.mjs:378, semantics/vm-extract/check.sh:16-22 — same issue when Charon/Aeneas is absent), apps/bench/windows (Cargo.toml:2 — no .test.contract currently), and tierTwoChecks() (scripts/async.mjs:79-88, 312 — cross-compiles without platform gate).
