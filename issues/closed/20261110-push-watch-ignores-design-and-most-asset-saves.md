# `push --watch` does not publish after a save to the design file, a font or a sound

**Status:** Closed
**Resolution:** The watcher asks `pushReads`, which takes the design (`.tamber`) and any file under `assets/` or `deck/` whatever its type, and skips `.exact/` with `target/` and a `.DS_Store`.
**Systems:** scripts/push.mjs (--watch)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`push --watch` "publishes after each save" (`scripts/push.mjs:30-31`). Its watcher drops
every file whose extension is not on a short list (line 259):

```js
if (!file || /(^|\/)(target|node_modules|\.git)(\/|$)/.test(file)
  || !/\.(contract|rs|toml|json|png|jpe?g|svg|mjs|ts|css|html)$/.test(file)) return;
```

A push reads more than that list:

- The plan carries the app's design (`app.tamber`, `contract/cli/src/sources.rs:300`), and
  the dev loop watches it as a source (`host/web/dev.mjs:618`). Six apps under `apps/` have
  one.
- `carriedAssets` (line 168) carries every regular file under `assets/` and `deck/`,
  whatever its type. The apps' `assets/` hold 22 `.ttf`, 6 `.wav`, 4 `.gif`, 4 `.heic`,
  3 `.mov`, 2 `.webp` and 2 `.mp4` files, none on the list.

A save to any of these is not published until a listed file is saved afterwards.

The list also lets through files a push does not read. An app `exact new` made keeps its
state under `.exact/`, and a write to `.exact/sent.json` (`scripts/feedback.mjs:170`) or
`.exact/shells/Cargo.toml` starts a push.

## The failure

With `bun scripts/push.mjs <app> --watch` running, save `app.tamber` with a changed token.
Observed: no `edit:` line and no push. Expected: `edit: app.tamber`, then the stream's next
`seq` carrying the new plan, as a save to `app.contract` gives.

## Verification

Ran the script's own `pusher` with `--watch` on an app directory, with the bake, the signer
and the publish replaced by doubles as `scripts/push.test.mjs` binds them, so the watcher,
its filter and the push queue were the script's and each push that started was counted.
Each file was then saved in turn:

| saved | `edit:` line | pushes started |
|---|---|---|
| `app.contract` | yes | 1 |
| `assets/logo.png` | yes | 1 |
| `app.tamber` | no | 0 |
| `assets/Inter.ttf`, `assets/chime.wav`, `assets/photo.webp` | no | 0 |
| `deck/deck.js` | no | 0 |
| `.exact/sent.json`, `.exact/shells/Cargo.toml` | yes | 1 each |

Compiling a copy of `apps/theme-fixture` before and after changing `accent.fill` in its
`app.tamber` gave two different plans (6,139 and 6,152 bytes), so the save changes what a
push would carry.

Not run: a watch against a deployed stream with a signing key.

## Direction

Watch what a push reads: add `tamber`, drop the extension list for files under `assets/`
and `deck/`, and skip `.exact/` with `target/`.

## Related

`issues/20261110-push-channel-rule-differs-from-bake-and-deploy.md` is about the channel
name, and `issues/20261110-serve-rebuild-misses-deep-sources.md` about `serve`'s rebuild.
