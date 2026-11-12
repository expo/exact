# `exact add` tells the author to run `bun exact.mjs add`, a verb an app's `exact.mjs` does not have

**Status:** Closed
**Resolution:** The app's exact.mjs already forwarded add (bcecabe0); its advice, the three diagnostics and the guide now spell it bun exact.mjs add, and its success line bun exact.mjs docs, verbs the app runs.
**Systems:** scripts/exact.mjs (add), scripts/app-cli.mjs (appVerbs), scripts/help.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`add` exists only in exact2's own `scripts/exact.mjs` (line 829). Run without an app it
refuses with advice (line 836):

```
exact: add: name the app (exact add geolocation <app>), or run bun exact.mjs add in it
```

An app's `exact.mjs` dispatches through `appVerbs` (`scripts/app-cli.mjs:64`), which has no
`add`, and the help index leaves the verb out (`scripts/help.mjs:576`). So the second half
of the advice fails. The other verbs an author is told about (`hatch`, `pattern`, `docs`,
`snapback`) are verbs of the app's `exact.mjs`, which is why an author tries this one that
way. Three diagnostics and a guide also say `exact add <module>` with no path,
though there is no `exact` on PATH: `contract/types/src/checks.rs:883`,
`contract/cli/src/vocab.rs:827`, `contract/cli/src/native.rs:676` and
`docs/reference.md:1627`.

## The failure

```sh
bun scripts/exact.mjs new /tmp/field-log && cd /tmp/field-log
bun exact.mjs add geolocation
```

Observed, exit 2:

```
Usage: bun exact.mjs <status|web|web-build|serve|test|agent|verify|ios|mac|share|linux|windows|android|update|lock|contract|tamber|hatch|snapback|backend|pattern|docs|feedback> [arguments for that script]
bun exact.mjs help says what each does.
```

Expected: `@exact/geolocation` added to `package.json` and installed, as
`bun <exact2>/scripts/exact.mjs add geolocation .` does from the same directory.

## Verification

Ran three commands. `bun <exact2>/scripts/exact.mjs add geolocation` from the checkout and
from inside a new app prints the advice above and exits 1. `bun exact.mjs add geolocation`
in the app prints the usage line and exits 2; `bun exact.mjs help add` prints `no verb add`.
`bun <exact2>/scripts/exact.mjs add geolocation .` in the app installs the package and
prints `add: @exact/geolocation is file:<exact2>/packages/geolocation. …`.

## Direction

Give the app's `exact.mjs` the verb: one row in `appVerbs` that runs `scripts/exact.mjs add`
with the app's directory, and `add` in the help index. The advice and the diagnostics are
then true. Otherwise change the advice, the three diagnostics and the guide to the form
that works, as `contract/cli/src/resolve.rs:292` and `docs/recipes/oauth.md:6` already do.
The success line's `exact docs a capability that asks permission` wants the same
`bun exact.mjs` spelling.

## Related

No issue, QUEUE entry or `docs/agent-pitfalls.md` line names this.
