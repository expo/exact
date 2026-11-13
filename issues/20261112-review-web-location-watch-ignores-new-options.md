# Web location watches ignore later readers' options

**Status:** Open
**Systems:** @exact/geolocation, web live resources
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the first `watchPosition` reader fixes the browser's
acquisition options until every reader leaves. A later reader, or an
existing reader with changed arguments, cannot request higher accuracy,
a different timeout, or a fresher position. It immediately receives the
shared cached answer even when that answer fails its own `maximumAge`.

`packages/geolocation/web/index.js:68–70` passes options to the browser
only when the single `watch` is absent. Line 72 returns `last` without
checking the new options, its timestamp, or the accuracy mode under which
it was acquired. LLP 1133.002 D2 requires written options to retain their
web meaning and permits a cached fix only within its requested age and
with matching `enableHighAccuracy`; D5 preserves each reader's options
when acquisition is shared.

A runnable probe imports the actual package and substitutes only
`navigator.geolocation`. The first reader asks for
`{enableHighAccuracy:false, maximumAge:600000}` and receives a cached
five-minute-old fix. A second asks for
`{enableHighAccuracy:true, maximumAge:0, timeout:0}`. Output on pinned
Bun 1.4.2:

```text
browserRegistrations: [{enableHighAccuracy:false, maximumAge:600000}]
preciseRequestAnswer.coords.accuracy: 3000
staleAgeMs: 300000
```

The second call succeeds with the first fix. It neither asks the browser
for high accuracy nor reports `TIMEOUT` for the absence of an eligible
cached fix. The probe is
`/tmp/exact-review-20261112/geolocation-options-probe.mjs`; no location
hardware or network was used. The defect is option and cache handling,
independent of how precisely a real browser can locate the device.

Track active option groups and compose or separately register their
acquisition requirements. Check cache eligibility per reader, and
reconcile acquisition when an argument changes or a reader leaves.
Validate two simultaneous readers with different freshness/accuracy
requirements, a reader whose options change, timeout zero with an
ineligible cached fix, and releasing the reader that requested the
stronger acquisition.

Checked all open and closed filesystem issues and the 275-issue GitHub
snapshot for `watchPosition`, geolocation options, accuracy, timeouts and
reader arguments. No matching issue was found. `QUEUE.md:438` records
related option composition work on Apple, but does not track this web
first-reader/cache failure.
