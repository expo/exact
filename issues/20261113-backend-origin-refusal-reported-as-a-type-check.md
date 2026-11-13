# A backend selection's refusal of an app.ts without `const origin` is reported as a failed type check

**Status:** Open
**Systems:** js/bake/src/typescript.mjs (`injectBackendOrigin`), host/web-js/build.mjs, scripts/exact.mjs (`typeDiagnostics`), scripts/help.mjs (`--backend`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-test-server-url-read-from-comments-before-settings.md

## What is wrong

`EXACT_BACKEND_ORIGIN`, `.exact/backend-origin` or a drive's `--backend <url>` points a build at
another backend by rewriting one top-level `const origin` in app.ts. Its value may be a literal or,
since settings are inlined first, `import.meta.env.NAME ?? '<literal>'` (`configure`,
`js/bake/src/typescript.mjs:888-892`). An app.ts without one is refused
(`injectBackendOrigin`, `:200-203`):

```
EXACT_BACKEND_ORIGIN: app.ts needs one top-level literal const origin; use it in net.fetch ${origin}
```

The selection runs inside the capture that the type check reads, and the web build reports any
error from it as the type check's (`host/web-js/build.mjs:309-312`):

```
error: <app>: app.ts failed the type check every host's build runs
```

That last line is what a builder reads first. `contract build --json` makes the refusal a
`typescript-check` diagnostic on app.ts (`typeDiagnostics`, `scripts/exact.mjs:758-770`). Nothing
says the backend selection caused it, and the refusal names `EXACT_BACKEND_ORIGIN` even when
`--backend` or `.exact/backend-origin` made the selection. It does not say that a setting may
supply the origin (`const origin = import.meta.env.API_BASE_URL ?? '…'`), so an app whose base
URL is a setting under another name reads it as unsupported. `--backend`'s help line
(`scripts/help.mjs:32`, listed by `test` and `agent`; `verify` takes the flag and does not
list it) says only "select the backend origin for this build", not which shape of app.ts it
needs.

## The failure

An admin app builder at `94b8671f7`, trying `EXACT_BACKEND_ORIGIN` after `API_BASE_URL` was
ignored by `test` (the related issue): the run "fails the type check with an unclear 'failed the
type check' line". Not reproduced here; the code path is as described at `49ae6357`.

## Fix

Report a backend-selection refusal as itself, naming what selected the backend
(`error: --backend: …`), not under the type check's summary line, and give it its own diagnostic
id. Name in the refusal and in `--backend`'s help that app.ts needs one top-level
`const origin`, a literal or `import.meta.env.NAME ?? '<literal>'`, used in `net.fetch ${origin}`.
