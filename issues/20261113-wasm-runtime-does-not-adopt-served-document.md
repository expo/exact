# Caltrain's wasm runtime does not adopt its served document

**Status:** Open
**Systems:** host/web (served document, adoption), host/web/tests/document.test.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

Five tests of `host/web/tests/document.test.mjs` fail against a development wasm build of Caltrain, all at the runtime's adoption of the served document (LLP 1048.000 D6):

- "Caltrain's served document is the live host's DOM" (line 328): the live tree differs from the served one in 248 entries, each a `data-exact-paint` attribute the served document has and the live one does not.
- "a press before the glue runs is captured, replays once, and a later one isn't doubled" (line 397), "a link that doesn't leave (a 204) stops the runtime's download, and the page downloads it again" (426), "Back from the bfcache downloads the runtime a link stopped" (443) and "the render server's page is the document, and the runtime adopts it" (478): `exact.agent({op:'state'}).adopted` is false.

## How to reproduce

At 69668888a on the async lane's Mac:

```
EXACT_WEB_LINK=all EXACT_WEB_DIST=/tmp/cw bun host/web/build.mjs caltrain-web --wasm
EXACT_WEB_DIST=/tmp/cj bun host/web/build.mjs caltrain-web
EXACT_REQUIRED=1 EXACT_WEB_DIST=/tmp/cw EXACT_WEB_JS_DIST=/tmp/cj bun test ./host/web/tests/document.test.mjs
```

Fails the same five in each of two runs; the file's other tests pass.
