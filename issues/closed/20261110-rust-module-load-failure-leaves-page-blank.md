# A Rust module that fails to load leaves a JS-target page blank, or without its data, and only the console says so

**Status:** Closed
**Resolution:** A Rust module that does not load is said once in the journal with the response's status or the failed fetch, a mixed app starts anyway, and each resource whose source the module owns fails naming the load.
**Systems:** host/web-js/build.mjs, host/web-js/rust-data.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd` the entry module that `host/web-js/build.mjs:512-514` writes loads an app's Rust module with no
handler for a failure (`mixed` is written in as `true` or `false`):

```js
const load = () => import('./rust-data.js').then(m => m.install(data, sources)).finally(() => inflight.n--);
inflight.n++;
if (wait || mixed) load().then(() => go(start)); else go(() => { start(); requestAnimationFrame(() => setTimeout(load)); });
```

- An app with both TypeScript and Rust sources (`mixed`) starts only after the module is installed. When
  `rust/wasm/app.module.wasm` or `app.bind.plan` cannot be fetched, `start` never runs: `#exact-root` stays empty and
  `exact.ready` is never set.
- An app with Rust sources only starts first. The page shows its compiled values, the sources never become ready,
  and nothing on the page or in the journal says why.

In both, the failure is an unhandled rejection in the browser's console. `install`
(`host/web-js/rust-data.js:70-73`) also passes a response's bytes to `WebAssembly.instantiate` without testing
`ok`, so a 503 or a 404 page is reported as `CompileError: WebAssembly.instantiate(): expected magic word 00 61 73
6d`. Nothing retries.

`wait` is the constant `false` (`host/web-js/src/emit.rs:806`), so `mixed` is the only way onto the first path.

## The failure

Build Update Lab, which has `app.ts` and a Rust module, and Caltrain, which has a Rust module only:
`bun host/web-js/build.mjs update-lab --out <dir> --render none`, and the same for `caltrain`. Serve the directory, answering
`/rust/wasm/app.module.wasm` with HTTP 503, or resetting that connection, and open the page in Chrome.

| app | module served | 503 | connection reset |
|---|---|---|---|
| Update Lab | 32 nodes, `exact.ready` true | 0 nodes, `exact.ready` undefined, `CompileError … expected magic word` | 0 nodes, `exact.ready` undefined, `TypeError: Failed to fetch` |
| Caltrain | 284 nodes, the station named | 52 nodes, no station, the same `CompileError` | 52 nodes, no station, `TypeError: Failed to fetch` |

Counts are the elements under `#exact-root` four seconds after the load. Expected: the page says the module did not
load (the journal, and a failed resource for each source the module owns), a mixed app still starts, and the error
names the response's status.

## Verification

Both builds were made at `dd6f5dcd` and opened in Chrome from a static server with the three behaviours above; the
table is what the pages reported. A failed fetch of `app.bind.plan` was not run.

## Direction

Catch in `load`: journal the failure once, test `r.ok` in `install`'s loader and name the status, start a mixed
app anyway, and fail the resources whose source the module owns (`rt.js` `giveUp` fails a resource this way for a
read a refused commit owed). `issues/closed/20260927-web-presence-load-failure-freezes.md` gave the presence module
a terminal "unavailable" state for the same reason.

## Related

Nothing in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` covers a failed load of the Rust module.
