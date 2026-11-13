# Apple border and content curves leave a transparent gap

**Status:** Closed
**Resolution:** `BorderPaint.contentRadii` reduces the content edge's radii in the content box, as the border hole's are reduced in the padding box and as Chrome reduces both (the hole's 80 was right; the content's 90 was not), so the two curves meet.
**Systems:** host/apple/Sources/ExactKit/BorderPaint.swift, host/apple/Sources/ExactKit/ContentBox.swift
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/apple/Sources/ExactKit/BorderPaint.swift:116–121` reduces the border hole's radii a second time against the inset rectangle. `contentRadii` at line 54 only reduces against the outer box before subtracting insets. The image clip at `host/apple/Sources/ExactKit/ContentBox.swift:62–64` uses those latter radii directly. The border and image consequently use different inner curves.

## The failure

Take a 100×100 box, a uniform 10-point border and only a top-right radius of 100. The content rectangle is `(10,10,80,80)`. Its top-right radius is 90, but the border hole rescales that radius to 80. Point `(45,20)` lies inside the hole and outside the content clip, leaving it in neither the border nor the picture. The two should meet without a gap.

## Verification

Ran `bun host/apple/build.mjs --test --only Fix08ReviewMacTests`. A temporary XCTest used the production `BorderPaint.paint`, `contentRadii` and `roundedRect`, painting the border red and content blue into a cleared bitmap with the view's downward y axis. It printed `content includes (45,20): false` and `border point (45,20): RGBA=[0, 0, 0, 0] content radius=(90.0, 90.0)`. The nontransparent-pixel assertion failed. The test does not require an on-screen animation. The temporary test was removed.

## Direction

Derive the border hole and content clip from the same outer-radius reduction and adjacent insets, without independently rescaling the inner border curve.

## Related

[Bordered rounded picture covers its border](20261110-bordered-rounded-picture-covers-its-border.md) concerns the shared painting host's picture clip. This is an Apple border-hole mismatch. Reported in the f-bordered-rounded-picture review.
