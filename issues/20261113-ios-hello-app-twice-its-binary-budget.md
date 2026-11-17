# The iOS hello app is twice its binary budget

**Status:** Open
**Systems:** host/apple (iOS binary size), scripts/metrics.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`bun scripts/metrics.mjs --long` measures the hello app's stripped iOS simulator executable over an empty UIKit app's (LLP 1047.001 D8) and gates it at `IOS_HELLO_BYTES`. Both rows are VIOLATIONs at 69668888a on the async lane's Mac (Xcode 27.2):

| | measured | budget |
|---|---|---|
| optimized for speed | 18,789,856 B (17.92 MB) | 8,600,000 B |
| optimized for size (the iOS app and embed default) | 12,319,936 B (11.75 MB) | 6,050,000 B |

The lane's run at afde7b25e measured 18,443,344 B and 12,103,872 B, so the binary grew by some 200–350 KB across those 545 commits as well.

## How to reproduce

```
bun scripts/metrics.mjs --long
```

The two `iOS: hello app` rows. The run takes hours on a loaded machine; the rows' build is `metrics.mjs`'s `IOS_HELLO_BYTES` section.

## Status (2026-11-13)

Still open: no regression large enough to close the gap was found, and what
remains needs a ruling.

**Measured** on one Mac (Xcode 27.2), `metrics.mjs`'s ios-size step run alone,
bytes over the floor. The budget's own commit reproduces its number (8,495,136
against the comment's 8,495,088), so these compare with it. Commits before
2026-11-01 are the private repository's.

| commit | date | speed | Rust text | Swift text |
|---|---|---|---|---|
| e6caf40bf (the budget) | 10-08 | 8,495,136 | 3.17 MB | 3.80 MB |
| c4d5e135bb | 10-16 | 10,252,832 | 3.76 MB | 4.62 MB |
| 636ff1263c | 10-17 | 11,035,728 | 4.06 MB | 4.95 MB |
| bffeda03f6 | 10-22 | 13,201,360 | 4.84 MB | 5.97 MB |
| f2447c136d | 10-23 | 13,568,704 | 4.98 MB | 6.16 MB |
| 8388cd0a48 | 10-28 | 16,290,928 | 6.55 MB | 7.07 MB |
| 029cd282d3 | 11-01 | 16,813,472 | 6.57 MB | 7.45 MB |
| 2a2f95a00 | 11-13 | 18,806,368 | 7.35 MB | 8.53 MB |

At 2a2f95a00 the size row is 12,352,944. Growth is steady, about 0.3 MB a day,
with no single jump: Swift added 4.7 MB and Rust 4.2 MB of `__text`.

**Where it went** (speed, budget to 2a2f95a00, by the linked symbols in the dSYM):

- `exact_runner` +1.88 MB. Of it, `runner::modules` (the native module seam, LLP
  1133) is 0.86 MB, nearly all added 10-23 to 10-28 as the `@exact/*` packages
  landed (payments, NFC, calls, notifications, clipboard, compose, background
  transfer and others). Hello's plan names no module, yet the runner reaches the
  seam from `commit::native_work`, `advance` and `settlement`, and through it
  `exact_grants`, `url`, `idna` and `exact_logic` (about 100 KB more).
- `ExactKit` and `ExactCore` +4.1 MB, across the core: `NodeView` +356 KB, `Agent`
  +219 KB (552 KB in all), `NavigationHost` +188 KB, `Presenter` +187 KB, `Runtime`
  +177 KB, `ExactSession` +114 KB, frame health's card and sampler 415 KB (LLP
  1079.000 ships it in every build), `HeadlessSession` 71 KB, the menus +127 KB.
- `exact_kernel` +525 KB (style, generated tables, layout, the arena), and
  `core` +521 KB, mostly monomorphized sorts (`slice::sort`, 225 to 455 KB, 470
  instances, 142 of them the runner's).

**Fixed:** the launch preloads (LLP 1162.000) were linked into every app: the
host held the concrete `LaunchPreloads`, whose drop and methods reach the
executor's preload cache, `url` and ibex2's grants. A session now holds it as a
`dyn LaunchWork` that only `IoLinks` makes. Speed 18,806,368 to 18,724,000 (−82
KB); size 12,352,944 to 12,319,552 (−33 KB).

**The ruling needed:** the rows are 2.2 and 2.0 times their budgets. Linking the
module seam by use, as LLP 1047.001 does for Markdown, surfaces and I/O, would
save about 1 MB (speed), but it is a seam through some 35 runner entry points,
not a regression fix. Frame health is in every build by LLP 1079.000; whether
the agent's 552 KB and the rest of the Swift growth belong in every production
app is the same question. Either the budget is re-based on what the core now
holds on purpose, or the module seam (and whatever else is ruled optional) goes
on the link-by-use list and the budget stays.

## Status (2026-11-14)

The delegated ruling removes the Swift agent from Apple's production
composition. Development compositions retain the agent and remain drivable.
The package excludes the agent implementation sources when
`EXACT_UPDATE_TRUST=production`; product helpers remain in `ExactKit`, and an
optional hook connects them to the agent only when that implementation is
present. Frame health remains in every composition and no longer reaches the
agent for native layout.

The ios-size step, run alone on this lane at base `e8f945f3e`, measured:

| composition | speed | size |
|---|---:|---:|
| before | 19,168,544 B | 12,520,000 B |
| after | 18,175,416 B | 11,736,984 B |
| reduction | 993,128 B | 783,016 B |
| budget | 8,600,000 B | 6,050,000 B |

These are stripped simulator executables over an empty UIKit executable; the
empty executable was 53,680 B before and 53,688 B after. Both rows remain over
budget. On the `a54f4234f` base, this composition measured 18,818,992 B at
speed, 12,019,600 B at size and 16,669,360 B balanced. Main's
`IOS_HELLO_BYTES` ceilings remain 8,600,000 B, 6,050,000 B and 16,900,000 B,
so the first two rows still exceed them. Whether those ceilings move is owed
to the maintainer; the composition change does not decide it.

The production link proof requires fresh build receipts and checks every iOS
and macOS production Hello bake it finds. Each contains the frame-health
positive-control symbol and no `ExactKit.Agent` symbol. Fresh development
bakes contain `ExactKit.Agent.startSocket` or `startStdio`, plus
`accessibilityElementsTree` and `layout`. A production Hello launched on a
simulator and showed its first screen, and its four-finger frame-health card
opened and rendered. A production macOS Hello also launched and showed its
text. Development iOS and macOS apps still answered agent trees and their
platform smokes exercised the same paths.

In a production bake, Save Trace returns `TraceError.production` and
`TraceSignal.install` does nothing, so neither the dev-menu command nor the
signal-triggered snapshot can write a trace. An authored
`startTrace_EXPERIMENTAL()` / `stopTrace_EXPERIMENTAL()` recording still works
in production. That path builds its frame report directly and never calls
`traceSnapshot()` or the agent's native-layout seam, so it has no layout field
to mark unavailable. The seam gives any future caller an explicit
`available: false` value whose reason says that native layout inspection is
not linked in a production composition.
