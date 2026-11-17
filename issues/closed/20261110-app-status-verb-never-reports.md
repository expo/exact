# An app's `status` verb is listed first in its help and can never report anything

**Status:** Closed
**Resolution:** `status` is a game's verb, as `prove` is, and an app's `web --ready` is refused before it starts anything, since only a game's dev loop and proof write the rows both read.
**Systems:** scripts/app-cli.mjs, scripts/help.mjs, game/status.mjs, host/web/dev.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Line references are at `dd6f5dcd`.

`bun exact.mjs help` in an app `exact new` made opens with

```
  status     web/prove status
```

and `bun exact.mjs status --help` says it "Lists web/prove invocations with their phase,
elapsed time, previous duration and exit status" (`scripts/help.mjs:79`). Only a game
writes those rows. `startStatus` is called by `buildingServer` (`game/dev.mjs:32`) and by
`game/prove.mjs:33`; an app's `web` is `host/web/dev.mjs` (`scripts/app-cli.mjs:67`), which
calls `buildingServer` only when the manifest declares `game` (`host/web/dev.mjs:108`), and
an app has no `prove` verb. So for an app the verb always prints the same line, and `web
--ready`, which reads the same rows (`scripts/app-cli.mjs:164-168`), can only time out.

## The failure

```sh
bun <exact2>/scripts/exact.mjs new hello && cd hello
bun exact.mjs web --port 8931 &        # prints http://127.0.0.1:8931/ and serves
bun exact.mjs status
bun exact.mjs web --ready --timeout 5
```

Observed while the dev loop is serving (`curl` of the URL answers 200):

```
No web or prove invocations recorded.                                  (exit 0)
web --ready timed out after 5s; run status for the current phase      (exit 1)
```

Expected: `status` names the running `web` with its URL, or an app's help does not offer
the verb. `web --ready`'s help line does say "game" (`scripts/help.mjs:86`).

## Verification

Ran the commands above in a new app outside the repository. `.exact/` after them holds no
`status.json`, the file `status` reads (`game/status.mjs:7`). `bun exact.mjs prove --help`
in the app prints the usage line, which has no `prove`.

## Direction

Either have an app's dev loop record its invocation as a game's does, or put `status` and
`--ready` among the game's own verbs in `appVerbs`, where `prove` already is.

## Related

`issues/20261110-top-level-help-omits-flags.md` is the `contract` CLI's help, not this.
