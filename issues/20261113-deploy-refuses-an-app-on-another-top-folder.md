# `deploy` refuses an app whose folder shares no top folder with the checkout

**Status:** Open
**Systems:** scripts/app-shells.mjs (generated `.exact/shells`), scripts/deploy.mjs (source capture)
**Severity:** P2
**Author:** Claude (Opus 5.5), found while fixing issues/closed/20261113-deploy-and-push-refuse-typescript-app-with-live-activities.md
**Date:** 2026-11-13

## What is wrong

An app made with `exact new` under `/private/tmp/…` (or any folder that shares no top folder
with the exact2 checkout under `/Users`) cannot be deployed: its generated `.exact/shells`
crates name exact2 by absolute path, because `pathFrom` makes a relative path only when the two
share a tree, and `deploy.mjs`'s source capture refuses a shell that names a path outside the
app. The same app under `/Users/…` deploys.

## Direction

Either let the capture accept the checkout's absolute path in a generated shell (it is the
build's own input, not the app's source), or say at `exact new` and in `deploy`'s refusal that
the app must live on the checkout's volume. Decide which by what LLP 1030.000's capture is for.
