# Every push downloads and verifies the stream's entire release history

**Status:** Open
**Systems:** scripts/deploy.mjs, scripts/push.mjs, scripts/origin.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `authenticatedReleaseFloor` lists every release record and
awaits a separate read and signature verification for each
(`scripts/deploy.mjs:860-885`). `nextOriginSeq` calls it even when the current
head is valid and authenticated (`:899`). Every changed `push`, including
every `--watch` publication, takes this path inside `publishStream`.

For an AppDrop stream with N releases, the next edit requires N serial
history GETs before uploading the changed files. The release history is
append-only, so the cost grows throughout the stream's lifetime and repeated
publication performs quadratic cumulative history work. A valid current head
does not make the normal path bounded. Deploy also scans records in
`latestRecord` during classification.

## Evidence

Ran the unchanged sequence-allocation functions with real Ed25519 signatures,
the real `inspectHead`, and an instrumented in-memory origin on Bun 1.4.2:

| Existing releases | History GETs | Maximum concurrent GETs | Next sequence |
| --- | --- | --- | --- |
| 10 | 10 | 1 | 11 |
| 100 | 100 | 1 | 101 |
| 1,000 | 1,000 | 1 | 1,001 |

The last case took 41 ms locally, excluding network time. At 30 ms per
completed GET, those 1,000 serial reads alone would take about 30 seconds;
that is a latency calculation, not a measurement of AppDrop.

The probe is `/tmp/exact-review-20261112/publisher-history-probe.mjs`. No
remote service or signing credentials were used; its key was generated for
the test and kept in memory.

## Fix and validation

Provide a bounded normal path for obtaining the authenticated sequence floor,
while preserving recovery from a rolled-back head and concurrent publishers.
Do not simply trust the current head or an unsigned cached maximum: the
history scan currently protects those cases. Test a long-lived stream's
request count as well as missing/corrupt/rolled-back heads and concurrent
publication. Bounded concurrency alone reduces latency but does not remove
the growing work.

Checked open and closed filesystem issues and all 275 GitHub issues fetched
for this review. No report of this publication-history scaling problem was
found.
