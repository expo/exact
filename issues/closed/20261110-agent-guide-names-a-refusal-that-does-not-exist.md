# docs/contract-for-agents.md names a refusal no compiler has (`lower-scroll-padding`), links a closed issue as open, and keeps two retired rules

**Status:** Closed
**Resolution:** The agent guide says any scroll container takes `scroll-padding`, links the closed repeating-hatch issue and keeps only UIKit's fixed-attachment limit, says a script `app.ts` never imports is no build input, and names six checks.
**Systems:** docs/contract-for-agents.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

| Line | The guide says | What is true |
|---|---|---|
| 1474-1478 | `scroll-padding`: "Only a virtualized list takes it (`lower-scroll-padding` elsewhere: native hosts read it nowhere else)" | No source spells `lower-scroll-padding`; the only other file with the word is the closed issue named here. `scroll-padding-top=44` compiles on a `scroll`, a `column`, a plain `list` and a virtualized one. `docs/contract-grammar.md:1771` says it "insets any scroll container's alignment port", and `issues/closed/20261009-scroll-padding-settlement-restore.md:42` records the change ("ordinary scrollers accept scroll-padding") |
| 1448-1450 | Repeating gradients: "UIKit qualification is pending … ([open issue](../issues/20261011-repeating-hatch-across-list-rows.md))" | The link is dead. The issue is `issues/closed/20261011-repeating-hatch-across-list-rows.md`, resolved "held to Chrome (UIKit included)"; its last section records the UIKit run of 2026-10-12 |
| 260-261 | A drive script is "kept outside the app folder, where it would count as a build input" | In an app that has an `app.ts`, as every app `exact new` makes has, a `.mjs` that `app.ts` never imports is no input (`scripts/agent-launch.mjs:862-866`, `930`). With `verify.mjs` added beside `app.contract` after a web build, the driver drives the app |
| 271 | "Use the existing five repository checks" | Six (`AGENTS.md`, "The six checks"; `docs/reference.md:2710`) |

The first row matters most: an agent told that `scroll-padding` is refused off a virtualized
list does not write it on a route's scroller under a fixed header, which is what it is for.

## The failure

```contract
component App
  view
    scroll scroll-padding-top=44 height=200
      text "a"
```

`contract build` accepts it. The guide says it is refused as `lower-scroll-padding`.

## Verification

Compiled `scroll-padding-top=44` on a `scroll`, a `column`, a plain `list` and a
`list virtualized=true` with `contract build`: each built. Searched the repository for
`lower-scroll-padding`: the guide's line and the closed issue are the only hits. Listed
`issues/` and `issues/closed/` for the linked file.

In an app made by `exact new`: ran `bun exact.mjs web-build`, added `verify.mjs` and
`evidence.json` beside `app.contract`, and ran
`EXACT_APP_DIR=$PWD bun <exact2>/scripts/agent.mjs web "state liked"`, which printed `false`
(the state). After an edit to `app.contract` the same command printed
`web build is stale: …/app.contract changed since …`. An app with no `app.ts` was not run:
there `moduleGraph` returns null (`agent-launch.mjs:887`) and every script counts.

## Direction

Rewrite the `scroll-padding` sentence from the grammar reference's; point the gradient
sentence at the closed issue and drop "UIKit qualification is pending"; delete the
parenthesis at 260-261 or limit it to an app with no `app.ts`; say six.

## Related

`docs/contract-for-humans.md:465` has the same `scroll-padding` sentence and
`docs/agent-pitfalls.md:1519` the same build-input rule; each belongs to the issue for its
own document. `issues/20261110-negative-scroll-padding-admitted.md` is about the compiler
admitting a negative `scroll-padding` value, not about which elements take the attribute.
