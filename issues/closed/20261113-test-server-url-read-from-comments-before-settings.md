# `test` and `verify` start the app's server at the first local URL in app.ts, a comment's included, over the setting's value

**Status:** Closed
**Resolution:** The server URL is a setting's local value first, else the first local URL in the code with comments blanked by the parser; when the code names several origins, the server's line says which it took and why.
**Systems:** scripts/verify.mjs (`serverUrl`, `appServer`)
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`appServer` starts `server.ts` at the URL `serverUrl` returns (`scripts/verify.mjs:146`).
`serverUrl` (`:104-107`) inlines app.ts's settings (`builtSettings`, `:99`) and then takes the
first match of `LOCAL` (`:75`, `http://127.0.0.1:<port>` or `localhost`) anywhere in the text.
It does not parse the module, so a URL in a comment counts, and a comment above the setting
wins over the setting's value.

Reproduced at `49ae6357` with `app.json` `{"env": ["API_BASE_URL"]}` and this `app.ts`:

```ts
// The server listens on http://127.0.0.1:8787 by default.
const API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8787';
export function load() { return fetch(`${API}/items`); }
```

`serverUrl(dir, { API_BASE_URL: 'http://127.0.0.1:9100' })` returns `http://127.0.0.1:8787`.
Without the comment it returns `http://127.0.0.1:9100`. So the value in the environment is
ignored without a word whenever any comment, string or earlier constant in app.ts names a
local URL, and the server is started (or a live one refused) at a port the app does not call.

## The failure

One admin app builder reported that three agents hit this independently at `94b8671f7`: a
comment naming the default port above `const API = import.meta.env.API_BASE_URL ?? '<default>'`
made `API_BASE_URL` in the environment silently ignored, so tests ran against, or were refused
because of, a different server than the app called.

## Fix

Take the URL from the parsed module rather than its text: the inlined value of the setting the
module calls, or a string literal, skipping comments. Where the text holds more than one local
origin, say which one the server is started at and why (the reporter asked that the
environment's value win and comments be skipped).
