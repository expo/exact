# `serve` does not rebuild for an edit more than four directories below the app

**Status:** Closed
**Resolution:** `rebuildOnChange`'s walk for the newest source descends without a depth bound, so an edit at any depth below the app builds the site again.
**Systems:** host/web/serve.mjs (`rebuildOnChange`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

For a build whose app's sources are beside it, `serve` builds the site again before a page when a source is
newer than the build, and says so at startup: "an edit to the app builds it again before the next page: leave
this running and reload" (`host/web/serve.mjs:1322`, at `dd6f5dcd`; `serve --help` and LLP 1153.001 say the
same). It finds the newest source with a walk that stops descending at a fixed depth (`:1376-1384`):

```js
const newest = (dir, depth = 0) => {
  …
    if (e.isDirectory()) { if (depth < 4) at = Math.max(at, newest(path, depth + 1)); } else if (e.isFile()) at = Math.max(at, statSync(path).mtimeMs);
```

Files are read in directories up to four levels below the app. A directory inside one at that level is not
entered, so an edit to a file five or more directories down changes nothing the walk reads. The page is served
from the old build and nothing is said. Nothing states the bound: the comment above the walk names only what
it skips.

## The failure

An app `exact new` made, whose `app.ts` imports `./a/b/c/d/e/part.ts` (five directories down) and
`./a/b/c/d/near.ts` (four), each exporting a string the greeting shows:

```
bun exact.mjs web-build --out dist
bun exact.mjs serve dist --port 18791
# edit a/b/c/d/e/part.ts, then twice, a second apart:
curl -H 'Accept: text/html' http://127.0.0.1:18791/
# edit a/b/c/d/near.ts, then the same request once
```

Observed: after the first edit both requests answer 200 in a few milliseconds, the log has no "building it
again" line and `dist/app.js` still holds the old string. After the second edit the log says
`a source changed since the build: building it again`, and `dist/app.js` then holds both new strings.

Expected: the first edit builds the site again, as the second does.

## Verification

Ran the commands above with the real build and `serve`; the results are as stated. No app in this repository
has a file that deep (the deepest tracked file under `apps/` is four directories below its app), and the app
`exact new` writes is flat, which is why this is P3.

## Direction

Walk without the depth bound: the names it skips already keep it out of `node_modules`, `target` and dot
directories.

## Related

None in `issues/` or `QUEUE.md`. `QUEUE.md`, "Web test freshness and prerequisites from the build", is about
the tests' freshness checks, not `serve`.
