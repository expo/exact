# `deploy.mjs` and `push.mjs` refuse a TypeScript app with widgets or Live Activities

**Status:** Open
**Systems:** scripts/deploy.mjs, scripts/push.mjs, bake/src/receipt.rs, bake/src/widgets.rs
**Severity:** P1
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-app-decides-update-never-staged-after-relaunch-on-ios.md, issues/closed/20261110-activity-arm-sources-resolve-against-cwd.md

## What is wrong

For an app with an `app.ts` and widget or activity entries, the Apple bakes
add the TypeScript module pair to the widget generation's assets:
`receipt::emit` passes `typescript = true` to `widgets::emit` only for `ios`
and `macos` (`bake/src/receipt.rs:83-87`), which adds
`typescript/app.hbc` and `typescript/app.module.json` to the graph
(`bake/src/widgets.rs:90-101`). The web bake, whose bundle the native streams
publish, has no such pair.

- **Deploy.** `deploy.mjs` compares each platform's graph with the bundle
  it read from the web bake and refuses on any difference
  (`scripts/deploy.mjs:1327-1330`): `ios bake graph differs from the packaged
  bundle`. Every deploy of such an app is refused.
- **Push.** Starting a new stream, `bakedAssets` reads each non-plan artifact
  of the binary's graph from the app's directory
  (`scripts/push.mjs:158-165`), where the bake-made `plans/`, `activities/`
  and `typescript/` files do not exist, so it refuses. And
  `typescriptBundle` appends the module pair to the graph and the bundle
  (`:126-143`) without removing the copy the graph already lists, so the
  pair would be listed twice.

## The failure

One agent, on exact2 `95f015045`, an iOS TypeScript app with
`live_activities`: `deploy.mjs` refused with `ios bake graph differs`
(`typescript/app.hbc` only in `ios`), and `push.mjs` could not start a new
stream. The agent patched both locally (reading bake-made activity assets
from the bake's output, and listing the pair once) to get an update out.

Confirmed at `49ae6357` by reading the code named above; not run.

## Fix

Make the published bundle carry what the native graphs carry: either the
deploy bundle includes the `typescript/` pair when a native platform's bake
has it (and the comparison holds per platform), or the comparison leaves out
the generation's assets that the bundle supplies another way. In `push.mjs`,
read bake-made assets (`plans/`, `activities/`, `typescript/`) from the bake's
output, not the app's directory, and have `typescriptBundle` replace an
existing `typescript/` artifact rather than append a second. Add a fixture
with `app.ts` and a Live Activity to `smoke.mjs deploy`.
