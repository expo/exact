# Apple's Hermes compact number formatter may ignore signDisplay always

**Status:** Closed
**Resolution:** Apple Hermes printed no sign for any `signDisplay` (`12K` for 12345 under `always`, `-0` under `exceptZero`); `Compact` now applies the sign after rounding, a plus where the engine prints its minus, and the pure-utilities fixture compares all four modes in four locales against Chrome.
**Systems:** js/src/intl.js, Apple Hermes
**Severity:** P3
**Author:** Claude (Opus 5.5), seen while fixing review-compact-formatter-retains-options
**Date:** 2026-11-13

While writing the Chrome comparison case for `Intl.NumberFormat(..., { notation: 'compact' })` (commit 'af3e9f7a1', js/src/intl.js `Compact`), the agent saw that on the Apple Hermes build a compact formatter made with `signDisplay: 'always'` printed a positive number without its `+`, where Chrome prints `+12.3K`. This was seen in passing and not reduced to a test.

## Direction

Add a `signDisplay: 'always'` (and `'exceptZero'`) compact case to the Chrome comparison fixture the compact formatter's test uses, run it on the Apple Hermes target, and make `Compact` apply the sign itself if Hermes does not.
