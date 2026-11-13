# `check` on the web reports `RoundButton`'s six light tints as contrast findings: only iOS's observer says a button is tinted

**Status:** Closed
**Resolution:** The web's `check` observer marks the label of a `buttonStyle="tinted"` button whose colour nobody wrote `tinted`, as iOS's does, so the rule's tinted exemption applies on the web.
**Systems:** host/web-js/check.js, scripts/agent-check.mjs (`floor`), host/apple/Sources/ExactKit/IOS/AgentCheckIOS.swift, contract/lib/content.contract (`RoundButton`), docs/card.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `dd6f5dcd`.

`RoundButton` draws green, mint, teal, cyan, yellow and orange in the platform's tinted
style, the hue over a pale fill of itself, as the Clock's Start is drawn
(`contract/lib/content.contract:258-263` and `:279-282`, LLP 1116.019 D3). `check`'s contrast rule leaves
that label out on purpose: "nor a tinted button's label in a system hue"
(`scripts/agent-check.mjs:55-56`), by `if (it.tinted && r && /^system-/.test(r)) return
null` (`:61`).

The rule reads a fact, `tinted`, that only iOS's observer sets
(`host/apple/Sources/ExactKit/IOS/AgentCheckIOS.swift:463`). The web's observer marks a
control's label `label`, `inactive` and `platformInk` (`host/web-js/check.js:228-236`) and
never `tinted`, so on the web the exemption cannot apply and each such button is a finding,
which `verify` counts.

`docs/start-here.md:24` names `RoundButton` for Start and Stop, and `docs/card.md:91` gives
its call with `tint=some("-exact-system-green")`.

## The failure

One `RoundButton(label="Go", tint=some("-exact-system-<hue>"), press=toggle,
disabled=false, testId="<hue>")` for each system hue, in an app `exact new` made:

```
$ bun exact.mjs agent web --size 390x1400 "check both"
check web · light: 6 findings
  contrast green's label "Go": 2:1 (#34c759 on #e6f8eb), needs 3:1; name a `color` for the label, or a fill it reads on in this scheme
  contrast mint's label "Go": 1.9:1 (#00c8b3 on #e0f8f6), needs 3:1; …
  contrast teal's label "Go": 1.9:1 (#00c3d0 on #e0f8f9), needs 3:1; …
  contrast cyan's label "Go": 1.9:1 (#00c0e8 on #e0f7fc), needs 3:1; …
  contrast yellow's label "Go": 1.4:1 (#ffcc00 on #fff9e0), needs 3:1; …
  contrast orange's label "Go": 2:1 (#ff8d28 on #fff1e5), needs 3:1; …
check web · dark: no findings
```

Red, blue, indigo, purple, pink and `tint=none` pass. `docs/recipes/content-design.md:42`'s
example (a green Start) ends `verify: 1 problem (1 finding; the tests passed)`. The remedy
printed cannot be followed: the label is the part's, and its only colour prop is `tint`.

Expected: `check web · light: no findings`, as the rule says and as
`host/web/agent-check.test.mjs:339-342` asserts of the judge.

## Verification

Ran the drive above and `verify` on the content-design recipe's blocks, on the web host.
With one line added to the web observer beside `platformInk` (a temporary edit, reverted),

```js
if (!inked && e.getAttribute('data-button-style') === 'tinted') item.tinted = true;
```

the same drive printed `no findings` in both schemes. Not run: iOS.

## Direction

Have the web's observer say `tinted` for a tinted button whose label colour nobody wrote,
as iOS's does.

## Related

`docs/start-here.md:15` warns of a pale accent under a filled button (`tamber-contrast`);
that is the filled style, which `check` still judges. Nothing in `issues/` or `QUEUE.md`
names `RoundButton` or the tinted exemption.
