# The JS page server follows a dist symlink and 500s on a bad percent

**Status:** Closed
**Resolution:** Already fixed by e0290101c, which made render.mjs --serve serve through serve.mjs (a decode that fails is no file, and a file whose real path is not itself is refused); a test now holds it.
**Systems:** host/web-js, host/render
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`render.mjs --serve` decodes `url.pathname` with `decodeURIComponent` and then serves any existing path under `dist` that has an extension. A bad `%` throws with no catch, so the fetch fails as 500. `existsSync`, `readFileSync`, and `Bun.file` follow a symlink whose path string stays inside `dist`, and the body can be a file outside `dist`. `path.resolve` only collapses `..`.

Rust `percent_decode` returns none for a bad percent, and `static_file` canonicalizes and requires the real path to stay inside `dist`. `host/web/serve.mjs` catches the decode and rejects a symlink.

## Where

`host/web-js/render.mjs`, the `--serve` fetch. `host/render/src/files.rs`, `static_file` and `percent_decode`. `host/web/serve.mjs`, the static fetch.

## How to reproduce

Request `/%` or `/%ZZ` from `render.mjs --serve`. The response is 500. Put a symlink named `leak.js` in `dist` pointing at a file outside the tree. The response body is that file.

## Direction

Catch the decode and answer 404. Realpath the file and refuse it unless it is a regular file whose real path is still inside `dist`.
