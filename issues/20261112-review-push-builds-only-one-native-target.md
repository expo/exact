# `push` builds only one native target for a channel with several targets

**Status:** Open
**Systems:** scripts/push.mjs, scripts/rust.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `scripts/push.mjs:222-225` selects the first native or tiered
cohort and calls `buildRust` once with its target. The publication loop then
uses that one result for every cohort (`:231-242`). `rustBundle` correctly
retains the produced module's target (`scripts/rust.mjs:476-494`), so the
classifier refuses the next architecture with a `rustTarget` mismatch.

An app with installed x86_64 and ARM64 Linux binaries can therefore update
only the architecture encountered first. The refusal says a new binary is
needed, even when the source change is compatible with both installed
binaries. The first stream may already have been published when the command
fails. The CLI has no target selector to work around this.

## Evidence

Executed the unchanged publisher functions extracted as in
`scripts/push.test.mjs`, with the real `rustBundle` and `classifyArtifacts`,
under Bun 1.4.2. Compilation and origin writes were stand-ins; no release was
published. Two otherwise compatible production cohorts produced:

```text
compiled: [x86_64-unknown-linux-gnu]
published: [x86_64-unknown-linux-gnu]
second cohort: app.plan: rustTarget (requires "x86_64-unknown-linux-gnu")
```

The probe is `/tmp/exact-review-20261112/push-probes.mjs`. This verifies the
publisher's orchestration and classification, not cross-compilation or a
device launch.

## Fix and validation

Build one variant for each required native target, as `deployCaptured` already
does, and select by the cohort's target. Validate candidates before starting
publication so a locally detectable incompatibility does not leave an
avoidable partial publish. Test two targets in both discovery orders and a
mixture of wasm, native and tiered cohorts.

Checked open and closed filesystem issues and all 275 GitHub issues fetched
for this review. The closed push format-digest issue concerns the candidate
plan's metadata, not selection of native module targets.
