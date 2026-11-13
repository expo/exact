# `deploy` does not check a TypeScript module pair's grants against an older stream

**Status:** Closed
**Resolution:** The bake graph's typescript/ entries require the binary's javascriptGrants exactly and its TypeScript runtime, so deploy's classifier routes a pair whose grants differ from an existing stream's binary to a new binary instead of a bundle the device refuses.
**Systems:** bake/ (the graph's `typescript/` entries), scripts/deploy.mjs, scripts/push.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), found while fixing issues/closed/20261113-deploy-and-push-refuse-typescript-app-with-live-activities.md
**Date:** 2026-11-13

## What is wrong

The bake graph records no requirements on its `typescript/` entries (`app.hbc`,
`app.module.json`). `push.mjs` checks a pair's grants exactly against the installed binary;
`deploy.mjs` classifies an Apple stream's bundle without them, so a deploy to an existing
stream whose binary holds narrower grants passes classification and is refused on the device.

## Direction

Record the pair's grants as the graph's requirement for those entries (as a Rust module's
requirements are), so the classifier refuses or routes to a binary as it does for Rust.
