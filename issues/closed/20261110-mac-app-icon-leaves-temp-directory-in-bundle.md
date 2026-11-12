# Every Mac bundle with a manifest icon ships an empty `.icon-XXXXXX` directory in its Resources

**Status:** Closed
**Resolution:** appIcon builds the Mac iconset under the OS temporary directory and removes it, so Resources holds only AppIcon.icns (LLP 1163)
**Systems:** host/apple/assets.mjs (`appIcon`), host/apple/bundle.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`appIcon` builds the Mac icon in a temporary `.iconset` (`host/apple/assets.mjs:41-48`):

```js
const set = mkdtempSync(resolve(dir, '.icon-')) + '.iconset';
mkdirSync(set);
…
run('iconutil', ['-c', 'icns', set, '-o', resolve(dir, 'AppIcon.icns')], { stdio: 'ignore' });
rmSync(set, { recursive: true, force: true });
```

`mkdtempSync` creates `.icon-XXXXXX`. The code then makes, fills and removes a second directory, `.icon-XXXXXX.iconset`. The first is never removed. `dir` is the bundle's `Contents/Resources` (`host/apple/bundle.mjs:106`, `appIcon(app, resources, 'macos')`), so the empty directory stays in the bundle.

## The failure

Any app whose `app.json` has a square icon of at least 512 px and no `host.macos.icon` (`apps/spark`, `apps/ui-gallery`, `examples/calendar`):

```
bun host/apple/build.mjs spark-apple --bundle
ls -a "target/clients/…/macos/Spark.app/Contents/Resources"
```

Observed: an empty directory `.icon-<six random characters>` beside `AppIcon.icns`, under a new name on every build, in development bundles and in the bundle `exact release` signs. Expected: `AppIcon.icns` and nothing left from making it, so that two builds of the same sources list the same files.

## Verification

Run. `apps/hello` with `"icons": [{ "src": "icon.png", "sizes": "1024x1024" }]` added to its manifest and a 1024 px PNG beside it, `bun host/apple/build.mjs hello-apple --bundle`, then `ls -la Hello.app/Contents/Resources`:

```
drwx------  .icon-rvalKl
-rw-r--r--  AppIcon.icns
-rw-r--r--  receipt.json
```

`codesign --verify --strict` passes on that bundle, and its `CodeResources` does not list the directory: an empty directory is not part of the seal, so the signature does not notice it.

`appIcon(app, <fresh directory>, 'macos')` called twice directly left `.icon-myxVg6/` and then `.icon-U4F2nY/`, each empty. The iOS branch returns before this code and makes no temporary directory.

## Direction

Make the iconset inside the `mkdtempSync` directory, or under the OS temporary directory, and remove that directory.

## Related

`issues/closed/20260927-launch-color-overwrites-icon-catalog.md` is about `actool` passes on iOS, not this directory. Nothing in `QUEUE.md` or `docs/agent-pitfalls.md` names it.
