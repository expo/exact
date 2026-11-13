# `push` discards freshly baked named plans and republishes the old ones

**Status:** Open
**Systems:** scripts/push.mjs, scripts/rust.mjs, widgets, live activities
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, a Rust producer returns newly baked `plans/` and `activities/`
files in `built.entries` (`scripts/rust.mjs:434`). `push` never incorporates
them. Instead, `carriedAssets` fetches those files unchanged from the previous
head (`scripts/push.mjs:181-185`); the candidate takes that list and adds only
the main plan and module pair (`:231-242`, `scripts/rust.mjs:476-495`).

Editing a widget or Live Activity's Contract and pushing therefore leaves its
old plan in the signed update. A push that changes the main app as well ships
the new app beside the old named entries. A named-entry-only edit can be
reported as already current. The watcher notices a `.contract` save, so
`--watch` does not remedy this.

The TypeScript path also reads only `app.plan`, `app.module.json` and
`app.hbc` out of its temporary bake before deleting the directory
(`scripts/push.mjs:109-117`). It needs the same complete-generation inventory.

## Evidence

Under Bun 1.4.2, executed the unchanged publisher functions using the extraction
approach in `scripts/push.test.mjs`, with real `rustBundle` and
`classifyArtifacts` and stand-ins for compilation and transport. A compatible
Rust cohort had an old `plans/widget.plan` in its head. Its producer returned a
new main plan and a new widget plan in `entries`:

```text
producer entries["plans/widget.plan"]: new widget plan
published bundle["plans/widget.plan"]: old widget plan
publisher error: none
```

The probe is `/tmp/exact-review-20261112/push-probes.mjs`. It verifies bundle
construction; it does not claim a WidgetKit device drive.

## Fix and validation

Publish named plans and their companion files from the same bake as the main
plan and data module. Replace the previous generation's named-entry inventory
rather than retaining removed entries. Test a changed, added and removed
widget/Live Activity entry for Rust and TypeScript, including an edit with no
main-plan change, and verify the asset index and module receipt agree.

Checked open and closed filesystem issues and all 275 GitHub issues fetched
for this review. `20261019-widget-refresh-native-hook-stale-layout-scale.md`
concerns the native refresh hook and binary-generation selection; it does not
cover the publisher retaining old entry bytes. GitHub #212 concerns widget
configuration, not update publication.
