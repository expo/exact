# Apple's min-content takes a run's end as a break, so a `break-word` paragraph splits a word that spans two runs

**Status:** Closed
**Resolution:** `minContentWidth` takes the line-break opportunities once over the paragraph's joined text, as layout does, and walks the pieces across runs (`TextEngine.paragraphPieces`), so a run's end inside a word is no break.
**Systems:** host/apple/Sources/ExactCore/Text.swift (`minContentWidth`), host/apple/Sources/ExactCore/TextBreaks.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `TextEngine.minContentWidth` finds the unbreakable pieces one run at a time (`host/apple/Sources/ExactCore/Text.swift:1306-1318`): it asks `lineBoundaries` for each run's own text, and the last boundary of any text is its length (`TextBreaks.swift:239`). So every run's end counts as a line-break opportunity. Layout asks for the boundaries of the joined text (`Text.swift:1071`), where a run's end inside a word is none. CSS agrees with layout: the boundary of an inline box introduces no soft wrap opportunity (CSS Text 3, "Line Breaking Details").

The two rules split on a word, or a word and its punctuation, that a styled span cuts: `See example.` with `example` bold is the runs `See `, `example`, `.`.

Under `overflow-wrap: normal` the answer comes out right anyway: the measurer lays the paragraph out at the short width (`Text.swift:1386-1399`), the whole word overflows, and the answer is the word's width. Under `break-word` the word breaks inside to fit the short width, so the wrong min-content is what the kernel gets. CSS counts no `break-word` break toward min-content (CSS Text 3 §5.5; `TextBreaks.swift:199-201` says the same).

## The failure

16 px system font, `overflow-wrap: break-word`:

```swift
func run(_ t: String) -> Run { Run(text: t, size: 16, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0) }
let s = Spec(runs: [run("See "), run("example"), run(".")], align: 0, lineClamp: 0, color: [0, 0, 0, 255], overflowWrap: 1, strut: run(""))
let min = engine.minContentWidth(s)       // 60.734; one run "See example." gives 65.172
engine.paragraph(s, width: min).lines     // "See ", "example", "."
```

| text | runs | min-content | laid out at it | Chromium 156 |
| --- | --- | --- | --- | --- |
| `See example.` | 1 | 65.172 | `See ` / `example.` | 65.172, 2 lines |
| `See example.` | `See `, `example`, `.` | 60.734 | `See ` / `example` / `.` | 65.172, 2 lines |
| `an unbreakable word` | 1 | 89.188 | `an ` / `unbreakable ` / `word` | 89.188, 3 lines |
| `an unbreakable word` | `an un`, `break`, `able word` | 40.547 | `an ` / `unbr` / `eaka` / `ble ` / `word`, width 35.969 | 89.188, 3 lines |

With `example` bold (weight 700) the three runs give 65.281 and the same three lines; the word and its full stop are 69.719 wide. A box the kernel sizes from min-content (a flex item shrunk to its automatic minimum, a shrink-to-fit box in a container narrower than the word) then leaves the full stop on a line of its own, or cuts the word.

## Verification

A temporary Mac XCTest (`bun host/apple/build.mjs --test --only <Class>`) called `minContentWidth` and `paragraph(_:width:)`, as the measurer does, and printed the Exact columns; under `normal` it printed 65.172 and 89.188 as the laid-out width for both splits. The Chromium column is a `width: min-content` block holding the same text as one `span` or three, in headless Chromium. Not run on iOS.

## Direction

Take the boundaries once over the joined text, as layout does, and walk the pieces across runs, skipping a hung marker's range and adding each piece's run indent as now.

## Related

`docs/agent-pitfalls.md`, "A long unbroken word runs off its box", explains why `break-word` adds nothing to min-content; it does not cover runs. No filed issue names `minContentWidth`.
