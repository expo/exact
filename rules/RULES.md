# Rules

Speed is the constraint. Keep the edit-to-verified loop under 30 seconds.

Each rule is **[check]** (enforced in CI) or **[review]** (enforced by a human).

## Budgets — every budget is a trade, not a limit

- **1 blocking check, 30s warm.** A second requires deleting this one, same PR. **[check]**
- **15 documents in the working set** (`llp/current/`). A 16th requires archiving one:
  removing its link. The corpus itself is uncapped. **[check]**
- **10 documents in the foundation** (`llp/foundation/`). **[check]**
- **This file: 700 words.** If it grows, something becomes a check or stops being a rule. **[check]**
- **1,500 lines per source file.** Generated files are exempt; they are built, not committed. **[check]**

A limit gets exceptions. A trade doesn't: it forces someone to name what matters less.

## Loop shape

- **Every check reports all failures in one run.** Fail-fast is banned. A check that
  surfaces one defect per run turns N bugs into N x runtime. **[check]**
- **Nothing blocks on anything slower than 30 seconds warm.** Slow verification runs
  hourly on fleet hardware, over the commits since the last run. New failures are
  bisected to the breaking commit.
  Run everything; block on almost nothing. **[review]**
- **Fix loops get 3 rounds.** Then stop and either escalate to a human decision or
  descope the target. Never "iterate until green." **[review]**

## Time budgets

Gate per change; other budgets hourly. Regressions are named P0s.

| | |
|---|---|
| Touch one line, rebuild that crate | 30s |
| Test what you changed | 60s |
| Blocking gate suite | 30s |
| Full build, warm cache | 5 min |
| Dev restart, request to present | 100ms p50 |
| Cold start to interactive first frame | 100ms p50 |
| App JS executed before first pixel | none; a launch that presents nothing (a wake, a system tool call) is exempt: LLP 1133.023 |

## Scope

- **`rules/DEFERRED.md` is binding.** Moving something onto the doing-list means writing why
  and taking something off, or Charlie's waiver. **[review]**
- **Delete; don't deprecate.** No compat shims, no migration paths, no legacy branches
  before 1.0. **[review]**
- **Write the web, ship the platform.** Authors write CSS; what they leave unsaid is the
  platform's, as a hand-built UIKit/SwiftUI app has it (LLP 1115, "no tells"). Author >
  platform > CSS default. **[review]**
- **The boot path executes and compiles nothing.** Modules ship as bytecode, never
  source strings transpiled per module at boot. Startup is emergent and cannot be
  retrofitted, so the boot graph is budgeted from commit one — counted, not timed.
  Every launch pays for boot. **[check]**
- **A spec needs an implementer and a date, or it isn't written.** Specifying something
  you are about to build is transcription and costs nothing. Specifying something nobody
  is assigned to build is how a corpus reaches 8.2M words. **[review]**

## Agents

- **Agents remove apparatus freely and add none.** An agent PR that adds a check,
  script, registry, config file, or design doc needs explicit human approval saying so.
  This asymmetry is the whole game: agents produce governance faster than humans can
  read it. Governance accretes one reasonable addition at a time. **[review]**
- **Prose follows `AGENTS.md` "Writing".** Before opening a PR or pushing to main,
  do an editing pass on the prose you added or changed. **[review]**
- **The old repo is research, never authority.** Cite it for how something worked.
  Never to block. **[review]**

## Fleet

- Warm, pre-provisioned worktrees, one lane each. Never clone fresh.
- **Never `git stash`**: the stack is global across worktrees and will apply another
  lane's work into yours. Use a temporary commit.
- Kill only PIDs you recorded at launch. Never by name or pattern.

## The gate (30 s)

`bun scripts/gate.mjs`

Format, caps, boot, incremental default-member checking and small unit suites.
Print timings and total; report all failures. The budget is warm after a
one-crate edit.

## The hourly lane

`scripts/async.mjs` checks origin/main hourly: the full checks, wasm, affected
suites, difftest, app drives, XCTests and platform builds. Every suite runs at
least daily. Bisect new failures with `affected.mjs`; obstructed probes report a
range without blame. Fix forward.
