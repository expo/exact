# docs/contract-grammar.md: productions the parser has outgrown, three inventories that are short, and stale counts

**Status:** Closed
**Resolution:** `docs/contract-grammar.md` gains the productions the parser takes (`timeline`, `color-profile`, a font weight range, timer options, `enum<…>`, the MIDI, wakelock and sound test steps, `clock held`), `time` in the tag table, the eleven missing handlers, the nine missing commands with a `setMode` row, and true counts and host statements.
**Systems:** docs/contract-grammar.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`. The grammar reference states each of these as the
whole of something ("names outside this table are not implicitly accepted", "the current
command name inventory", "all 61 handler names"). The parser and `contract vocab --json`
disagree.

| Line | The document | The tools |
|---|---|---|
| 96-97 | `declaration` lists fourteen forms | The parser also takes `timeline Name` and `color-profile --name src="…"` at top level (`contract/syntax/src/parser.rs:550-559`); a file that starts with either compiles |
| 119 | `font-face = NUMBER [ "italic" ] "=" STRING` | A weight range compiles: `100 1000 = "assets/RobotoFlex.ttf"`, as `docs/contract-for-humans.md:1092` writes it |
| 302-303 | `schedule` is `every(expr \| frame, IDENT)` or `after(expr, IDENT)` | Both take `wait=frame\|update\|scrollend\|idle`, `max=` and `tolerance=` (`docs/reference.md:3016-3043`); `every(5000, tick, wait=scrollend, max=1000)` and `after(5000, tick, tolerance=500)` compile |
| 596-597 | `type-atom` has no enum | `status: enum<"draft", "published">` compiles as a shape field and as a `fn` parameter; line 1172 describes it |
| 681-721 | The test grammar's `step` | The parser also takes `expect wakelock held\|released`, `expect sound … playbackRate N pan N`, `tap midi "keys" 90 3c 64`, `tap midi "keys" clock 125 2000`, `expect midi sent\|missing …` and a `prefer` of several `midi.*` pairs; the prose at 957-979 describes them |
| 989 | "The nine-operation interactive API" | Ten (`docs/contract-for-agents.md:3780`, `AGENTS.md`) |
| 1191-1204 | The tag table, "names outside this table are not implicitly accepted" | `time` is a tag (`contract vocab time`; `docs/reference.md:3056-3065`) and is in no row |
| 1219 | The card is "at most 8 KB" | `contract vocab \| wc -c` is 12,287; the bound is 12 KB (`contract/cli/tests/it/vocab.rs:715`, `docs/contract-for-agents.md:315`) |
| 1607 | "The table contains all 61 handler names" | The vocabulary has 84 handlers. The table (1660-1692) names 73 and omits `atompress`, `capture`, `currententrychange`, `download`, `enterpictureinpicture`, `leavepictureinpicture`, `navigateerror`, `navigatesuccess`, `newwindow`, `toggle` and `userpromptopened` |
| 1611 | "Six events bubble", then seven names | `key`, `keyup`, `focusin`, `focusout`, `pointerdown`, `pointerup`, `wheel` |
| 1666 | `intersect` is "currently web only" | AppKit, UIKit and Linux sample it natively (`runner/src/geometry/intersection.rs:1-2`; `docs/contract-for-agents.md:4582-4583`, `docs/reference.md:2985-2987`) |
| 2673-2681 | "The current command name inventory", 48 names | The vocabulary's 57 commands also hold `navigate`, `traverseHistory`, `stopLoading`, `captureScreenshot`, `openDevTools`, `setMode`, `showPopover`, `hidePopover` and `togglePopover`; the table under the list describes all of them but `setMode` |
| 2721 | "The web (its JS target) and the Apple hosts carry every command" | The table above it says Apple does not take `scrollBy` (2712; no file under `host/apple` names the command) and the web refuses `setPageZoom` (2703), `setWindowOpacity` (2704) and the trace commands (2699) |

Line 3 dates the document to "the compiler on `main` on 2026-10-02"; it cites rulings as late
as 2026-11-10 (line 1799).

## The failure

Each of these is outside the grammar as written and compiles:

```contract
timeline Scrub
shape Post
  status: enum<"draft", "published">
component App
  state n = 0
  action tick
    n = n + 1
  task t mount
    every(5000, tick, wait=scrollend, max=1000)
  view
    time datetime=1767225600000 format="date-long"
```

## Verification

Ran `contract build` on the program above and on a file for each production row (a
`timeline`, a `color-profile`, a font range over a real variable font, the timer options, an
enum field and parameter, the `time` tag, and an action calling `navigate`,
`traverseHistory`, `stopLoading`, `showPopover`, `hidePopover` and `togglePopover`) and
`contract test` on a test file for each step form; all were accepted, and
`every(5000, tick, banana=1)` was refused as `lower-timer-wait`, which names the three
options. Compared the tag table, the handler table and the command list with
`contract vocab --json` by script: the counts and names above are its output. Measured the
card with `wc -c`.

Row 1666: `cargo test -p exact-linux --lib native_intersect` passes (2 tests, the Linux
presenter delivering `intersect` at mount and on scroll). Row 2721: in an app made by
`exact new`, an action calling `setPageZoom(1.5)`, `setWindowOpacity(0.5)` and
`startTrace_EXPERIMENTAL()` was tapped on the web, and `logs` printed
`setPageZoom refused: the browser's zoom is the person's; a page cannot set it`,
`setWindowOpacity refused: a page cannot change its window's opacity` and
`startTrace_EXPERIMENTAL refused: the web host does not record traces; …`. The `scrollBy`
half was read: the Linux and terminal hosts dispatch `"scrollBy"`
(`host/linux/src/presenter/commands.rs:120`, `host/terminal/src/host.rs:437`) and nothing
under `host/apple` does.

## Direction

Add the missing alternatives to the five productions, `time` to the tag table and the missing
names to the handler table and command list; drop the counts ("61", "Six", "nine", "8 KB") or
correct them; make the sentence at 2721 say what the table says.

## Related

`issues/20261110-request-step-refuses-scalar-json-body.md` is the opposite case at line 704,
where the grammar admits what the parser refuses; it is not repeated here.
