# The agent driver takes a valued flag with no value as the flag's default

**Status:** Closed
**Resolution:** `parseFlags` reads every valued flag through one check that refuses, by the flag's name, a value that is absent or another flag (help.mjs `missingValue`), before anything is opened.
**Systems:** scripts/agent-launch.mjs (`parseFlags`), scripts/agent.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`parseFlags` reads each valued flag as `flags.x = argv[++i]` (`scripts/agent-launch.mjs:452-509`).
When the flag is the last argument the value is `undefined`, which every reader of the flag takes
as "not given". Only `--backend` (`:454`), `--size` (`:457-461`), `--process` and `--login-env`
(`:512-518`) refuse a missing value. So a drive whose command names a destination or a fact and
drops its value runs with the default instead of being refused:

- `--app`: the default app (`EXACT_APP_DIR`'s, else Caltrain, `scripts/app.mjs:499`);
- `--url`: this checkout's build, not the page that was to be named;
- `--storage`, `--port`, `--browser`, `--sim`, `--phone`, `--session`: the defaults;
- `--epoch`, `--locale`, `--format-locale`, `--time-zone`, `--timing`, `--touch`, `--chrome`,
  `--bind`, `--fail-fetch`: the launch facts and modes of a drive that named none.

## The failure

In an app `exact new` made, with a web build there:

```
for f in --app --url --storage --port --browser --sim --phone --session --epoch --locale \
         --format-locale --time-zone --timing --touch --chrome --bind --fail-fetch; do
  bun exact.mjs agent web "state liked" $f; echo "exit $?"
done
```

Observed: each of the 17 prints `false` and `exit 0`. In the repository,
`bun scripts/agent.mjs web "tree change-station" --app` drives Caltrain and exits 0.

For comparison `--size` exits 1 with `--size: expected positive <w>x<h>…`, `--seed` with `seed: a
whole number…`, and `--plan`, `--world`, `--open` and `--out` with the runtime's `The "paths[0]"
property must be of type string, got undefined`, which names no flag.

Expected: `--url needs a value` and the like, before anything is opened, as `deploy.mjs` refuses
(`scripts/deploy.mjs:70-72`) and `push.mjs` has since
`issues/closed/20261102-push-missing-flag-values-use-defaults.md`.

A flag in the value's place (`--app --json`) is taken as the value; most of those are then refused
by the value's own check (an app, a locale, a store name), with a message about the value.

## Verification

Ran the loop above against a worktree at `dd6f5dcd`, and the comparison flags the same way; the
results are as stated. In the repository, `--app`, `--url`, `--epoch`, `--storage`, `--locale` and
`--port` last on a Caltrain web drive each exit 0.

## Direction

One check where a valued flag reads its value: refuse `undefined` or a word that starts with
`--`, naming the flag.

## Related

`issues/closed/20261102-push-missing-flag-values-use-defaults.md` fixed this in `push.mjs` only.
`issues/20261110-build-flag-missing-value-taken-silently.md` is the same gap in
`host/apple/build.mjs` and proposes the check in `guardFlags` (`scripts/help.mjs:614`). That check
would reach `bun exact.mjs agent` and `test` on the web, iOS, macOS and Windows, where
`scripts/app-cli.mjs:326` passes `guardFlags` the raw arguments. It would not reach the driver run
directly or for Linux and Android: there `parseFlags` has consumed the flags before `guardFlags`
sees what is left (`scripts/agent.mjs:1347-1348`).
