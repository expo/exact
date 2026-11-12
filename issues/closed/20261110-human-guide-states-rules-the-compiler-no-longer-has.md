# docs/contract-for-humans.md states eight rules the compiler and hosts no longer have

**Status:** Closed
**Resolution:** `docs/contract-for-humans.md` states the eight rules as the compiler and hosts have them: every top-level form, exponent literals, options typed for derives too, a warned read-after-assign, shadowable standard functions, `scroll-padding` on any scroller, Linux's `focus`, and `frame()` with transforms.
**Systems:** docs/contract-for-humans.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`. These sentences are false, and the grammar
reference or the agent guide says the opposite of most.

| Line | The guide says | What happens | The other guide |
|---|---|---|---|
| 121 | "The top-level forms are `component`, `shape`, `fn`, `style`, `keyframes`, `font`, `routes`, `use`, and `test`" | The parser also takes `sound`, `theme`, `decoration`, `atom`, `timeline`, `color-profile` and `members` (`contract/syntax/src/parser.rs:526-599`) | grammar 96 |
| 181 | "exponent notation … not the language's literal forms" | `derive big = 1e3` compiles and is 1000 | grammar 43-45 |
| 253-255 | An option state typed by a later assignment: "derives are typed first: match such a state in the view, not in a derive" | `state picked = none`, an action assigning `some(3)`, and `derive label = match picked { case some(n) => … }` compile, and `label` is `"3"` after the action | |
| 317-319 | A helper reading what its caller assigned: "the compiler refuses such a read" | It compiles with the warning `analyze-read-after-assign` | grammar 2824-2836 ("no longer a refusal"), pitfalls 955-960 |
| 353-354 | "Standard-function names are reserved against redefinition" | `fn round(n: number): number = n + 100` compiles and shadows the roster's in free calls (`round(1.5)` is 101.5); `fn first`, `fn max`, `fn path` and `fn length` compile too | grammar 997-999, agents 454-456 |
| 460-465 | `scroll-padding`: "Other elements refuse it" (than a virtualized list) | `scroll scroll-padding-top=44` and `column scroll-padding-top=44` compile | grammar 1771 ("any scroll container") |
| 1177 | "headless Linux carries neither `focus` nor `openURL`" | The Linux host handles `focus(id)` (`host/linux/src/presenter/commands.rs:107`). `openURL` is unsupported there, as the sentence says | grammar 2721-2725 lists what Linux lacks; `focus` is not in it |
| 1523-1525 | `frame("id")` reads the box "without transforms" | Transforms are applied, the box's own and every ancestor's (`runner/src/geometry.rs:8-13`) | grammar 1072 ("every scroll offset and transform applied") |

## The failure

```contract
fn round(n: number): number = n + 100
component App
  state picked = none
  derive label = match picked { case some(n) => `${n}`, case none => "none" }
  derive big = 1e3
  derive r = round(1.5)
  action pick
    picked = some(3)
  view
    scroll scroll-padding-top=44 height=200
      button label press=pick testId="pick"
      text `${r} ${big}`
```

Rows 181, 253, 353 and 460 each say a line of this does not compile. `contract build` accepts
it with no warning.

## Verification

Compiled a case for each of rows 121 to 460 with `contract build`. Evaluated the program
above, with the helper of row 317 added, by `contract verify app.contract --runner-only
--walks 0` against a test expecting `big == 1000`, `r == 101.5`, `label == "3"` after a tap on
`pick`, and the helper's read still 0 after its caller assigned 5: it reported the test whole
with no failed step, and reported `State("r", …) does not hold` when the expectation was
changed to `r == 2`.

Row 1177: `cargo test -p exact-linux --lib a_node_shown_by_the_same_commit_takes_focus_before_its_picture`
passes (an action's `focus("late")` moves the Linux presenter's focus). Row 1523:
`cargo test -p contract --test it a_read_sees_the_box_through_every_transform_above_it`
passes (a card under a lane with `translate` and `scale=2` reads `frame("card").x` as -40).

## Direction

Correct the eight sentences from the grammar reference's text.

## Related

`issues/closed/20261009-scroll-padding-settlement-restore.md` is the change that admitted
`scroll-padding` on any scroller; `docs/contract-for-agents.md:1477` has the same stale
sentence, which belongs to the issue for that guide. No open issue covers the human guide.
