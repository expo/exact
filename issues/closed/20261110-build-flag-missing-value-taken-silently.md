# A value-taking flag of `build.mjs` given no value takes the next flag, or a default, without a word

**Status:** Closed
**Resolution:** `guardFlags`, which every script an app's verb runs calls first, refuses a flag whose spec names a value when the value is absent or another flag, and `appleBuildRefusal` refuses a `--link` value other than `plan`, both before anything is built.
**Systems:** host/apple/build.mjs, host/apple/link.mjs, scripts/help.mjs (`guardFlags`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`guardFlags` refuses an unknown flag, but for a flag declared with a value (`--archive <out.ipa>`, `--sim <udid|name>`, `--phone <udid|name>`, `--optimize <speed|size>`) it skips the next argument without looking at it (`scripts/help.mjs:614`). `build.mjs` then reads `args[index + 1]` each time (`host/apple/build.mjs:630`, `:674`, `:730`, `:1362`; `host/apple/link.mjs:80-82` for `--link`):

```js
const ipa = args.includes('--archive') ? resolve(process.cwd(), args[args.indexOf('--archive') + 1] ?? '') : null;
```

## The failure

```
bun host/apple/build.mjs --device caltrain-apple --archive --unsigned
```

Observed: accepted. `ipa` is `<cwd>/--unsigned`, so the archive, its `.dSYM`, `.dSYMs` and `.receipt.json` are written under that name. With `--archive` last, `ipa` is the working directory itself and the build stops only at its end, at `rmSync(ipa, { force: true })` (`:1356`), which throws `ERR_FS_EISDIR`.

The others:

- `--sim` or `--phone` last: `EXACT_SIM` or `EXACT_PHONE`, else the default choice, so the build installs on a device the command did not name.
- `--optimize` last, on iOS: the manifest's value, else the default. On macOS, where `build.mjs` appends `--bundle` (`:623`), it is refused as `--optimize is speed or size, not --bundle`, naming a flag the command did not have.
- `--link` followed by anything but `plan` (`--link all`, or last): a development build links every capability, as if the flag were absent (`linksByPlan`).

Expected: each refused before anything is built, as `--url` with no URL, `--only` with no class and `--watch-sim` with no UDID already are.

## Verification

Run. `guardFlags('ios', …)` returned each of these unchanged: `["--device","--archive"]`, `["--device","--archive","--unsigned"]`, `["--ios","--sim"]`, `["--ios","--optimize"]`, `["--ios","--link","all"]`. Then:

```
bun host/apple/build.mjs --ios activity-fixture-apple --no-install --optimize     exit 0
bun host/apple/build.mjs --ios activity-fixture-apple --no-install --link all     exit 0
bun host/apple/build.mjs --ios activity-fixture-apple --optimize --no-install     host/apple: --optimize is speed or size, not --no-install
bun host/apple/build.mjs hello-apple --optimize                                   host/apple: --optimize is speed or size, not --bundle
```

Not run: a device archive, or `--sim` and `--phone`; what `build.mjs` does with them is read from the lines cited.

## Direction

In `guardFlags`, which already knows each flag's spec, refuse a flag whose spec names a value when the next argument is absent or starts with `--`; and have `linksByPlan` refuse a `--link` value other than `plan`.

## Related

`issues/closed/20261102-push-missing-flag-values-use-defaults.md` fixed the same gap in `scripts/push.mjs`. `guardFlags`, which the web and Linux builds, the driver, `share` and `verify` also call, was not changed. No QUEUE entry names it.
