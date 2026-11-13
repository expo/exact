# Updating a Snapback app writes, then fails when the client is absent

**Status:** Open
**Systems:** game
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`createApp` throws `NO_SNAPBACK` before any write when `snapback4/` is missing. `updateApp` writes `exact.mjs`, the toolchain, and platform files first, then `refreshManifestPaths`. When `typescript.sources.snapback4` is set, that rewrite calls `driverMount`, which `realpathSync`s `snapback4/ts` or `snapback4/online`. The missing directory throws inside `String.replace`, so `app.json` is not written and `updateManifests` never runs. The schema path stays stale, and an authored app's exact2 path dependencies are never rewritten.

## Where

`game/new.mjs`, `updateApp`, `refreshManifestPaths`, `driverMount`. `createApp` already refuses before writing.

## How to reproduce

Update an app whose manifest points at Snapback, in a checkout with no `snapback4/` directory. The command throws `ENOENT` after writing `exact.mjs` and the toolchain files.

## Direction

If `snapback4/` is absent, skip the mount rewrite and report `NO_SNAPBACK`, or throw that before any update write. Still rewrite `$schema`.
