# docs/agent-pitfalls.md: a stale-build entry and a large-title sentence no longer hold, two entries give a wrong cause, and seven describe what the compiler now warns of

**Status:** Closed
**Resolution:** The pitfalls list drops the entries the compiler now warns of (keeping, for px widths and `width="100%"` with padding, only what the warning misses), says a button is a block that shrinks to fit, and states which app-folder files are build inputs; the large-title sentence stays, because `PushedTitleIOSTests` fails on an iOS 27.0 simulator on this commit (a pushed level-1 route's bar is 54 points, not 106).
**Systems:** docs/agent-pitfalls.md, docs/contract-for-agents.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`. The list's header (lines 3-6) says: "Delete the
entry in the change that fixes the footgun, or that makes the compiler, runtime or driver
diagnose it."

**No longer true.**

| Entry | It says | Now |
|---|---|---|
| 1519-1529, "A drive script kept in the app folder makes the build stale" | Editing `verify.mjs` beside `app.contract` makes the driver refuse the next drive; "any `.json` counts" | In an app that has an `app.ts`, as every app `exact new` makes has, a `.mjs`, `.ts` or `.json` file that `app.ts` never imports is no input (`scripts/agent-launch.mjs:862-866`, `930`). After a web build, `verify.mjs`, `evidence.json` and `helper.ts` added beside `app.contract` leave the build fresh. A file of another kind still counts: `drive.py` there is refused as stale. `docs/contract-for-agents.md:260-261` repeats the old rule ("where it would count as a build input") |
| 473-475, inside "A pushed screen's content is cut off…" | "A pushed route's large title opens collapsed over its scroller and expands on a pull (QUEUE.md); keep large titles for a stack's first screen" | It arrives expanded: `QUEUE.md:586` ("arrives expanded now"), LLP 1115.000's as-built row for D11.1, and `host/apple/tests/ExactKitTests/PushedTitleIOSTests.swift:184` asserts it |

**The cause is wrong.** Entry 176-181 ("A list of buttons is ragged") says "as on the web, a
button is `inline-block`", and entry 830-835 ("A glyph sits at the left of its round
button") says "a `button` is `inline-block`". Exact's button is block-level and shrinks to
fit, as entry 243-248 and `docs/contract-grammar.md:1295` say ("it is block-level, not
`inline-block`, so buttons in a block parent stack"). Two buttons in a 400-point `scroll`
measure 77.13 and 67.61 wide at y 86 and y 122: stacked, not side by side. Each entry's fix
is right.

**Now diagnosed.** The compiler warns at each entry's example and the warning names the
repair, so the header's rule deletes the entry. Entry 222 is marked "**Diagnosed:**" and
kept; the others but 939 name their warning.

| Entry | Warning (reproduced) |
|---|---|
| 60, a px `width` beside role text | `lower-text-px-width` |
| 111, `line-height=28` | `lower-line-height-multiplier` |
| 145, a symbol's `-exact-tint-color` | `lower-symbol-tint` |
| 222, `width="100%"` with padding | `lower-width-padding` |
| 887, `nav` read after it is assigned | `analyze-read-after-assign` |
| 939, resetting a mutation beside its send | `analyze-send-assigned` |
| 955, a helper reading what its caller assigned | `analyze-read-after-assign` |

## Verification

Compiled each diagnosed entry's example with `contract build`; each printed the warning
named. Entry 168 (a root with `width="100%"` and `padding=24`) stays: the compiler says
nothing of a root, which measured 1248 wide in a 1200-point window.

In an app made by `exact new`, on the web (`bun exact.mjs agent web --size 1200x900
"layout <id>" …`): the two buttons' boxes above, and a `width="100%" padding=16` column in a
400-point parent at 432 wide. The stale-build case: after `bun exact.mjs web-build`, with the
three files added, `EXACT_APP_DIR=$PWD bun <exact2>/scripts/agent.mjs web "state ticks"`
printed `0`; with `drive.py` added it printed `web build is stale: drive.py changed since …`,
and after an edit to `app.contract` the same for `app.contract`. An app with no `app.ts` was
not run: there every script counts (`agent-launch.mjs:865`, `887`).

The large-title sentence was read, not run: the iOS simulator does not run on this machine.

## Direction

Rewrite entry 1519 to what still counts, and the agent guide's parenthesis with it; drop the
large-title sentence from the entry at 468; correct the cause in the entries at 176 and 830;
delete the diagnosed entries, or shorten each to what its warning does not say.

## Related

No open issue or QUEUE entry covers these.
