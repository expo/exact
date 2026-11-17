# A failed text-flow module load leaves the wasm host without a clock scheduler

**Status:** Closed
**Resolution:** The ordinary ticker keeps the clock until a text-flow controller is ready, and a failed load is logged, clears its pending state, starts the ticker if none runs, and is tried again only in the next generation.
**Systems:** host/web
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, mounting a text-flow context disposes the wasm web host's
ordinary ticker before its replacement has loaded
(`host/web/wasm/flow.js:18-24`). If importing `textflow-glue.js`, fetching or
compiling `textflow.wasm`, or creating its controller rejects, the catch only
logs the error (`flow.js:60-64`). `host.ticker` and `host.textflow` remain null,
and `host.flowLoading` permanently holds the rejected promise.

This stalls ordinary application timers and scheduled frames along with text
flow. Later batches update the stored deadline but cannot deliver it to a
scheduler, and skip another load because `host.flowLoading` is truthy
(`flow.js:13-19`). The ordinary ticker's `startClock` also refuses to recreate
it while `flowLoading` is set (`host/web/wasm/boot.js:67-85`). Recovery requires
resetting the host generation. A transient failure loading an optional layout
artifact should not leave unrelated application timers suspended indefinitely.

**Reproduction.** Executed the unchanged `flowBatch` and `startClock` function
bodies with a controlled rejected loader, a disposable existing ticker, and
an inert timer factory. After the rejection, calling `startClock` and applying
a second batch with a new timer deadline produced:

```json
{"loads":1,"disposals":1,"created":0,"ticker":null,"textflow":null,"loadingStillSet":true,"timerDue":100}
```

The only log was the controlled module-load error. The probe is
`/tmp/exact-review-20261112/web-flow-load-failure.mjs`, run with pinned Bun
1.4.2. It exercises the actual ownership transition with inert dependencies;
no browser or network request was used. The report is scoped to the wasm host,
which remains used by games and internal wasm builds.

**Direction.** Transfer clock ownership only after the text-flow controller is
ready, or restore the ordinary scheduler on failure. Clear the failed pending
state within the same generation and define the retry behavior. Validate a
failed import and a rejected controller creation while an ordinary timer is
pending, then a later successful load, with one scheduler owning each phase.

**Duplicates checked.** Searched open and closed filesystem reports and all
275 cached GitHub issues for text-flow loading failures, ticker ownership and
timer loss. The closed wasm-opt report concerns a build failure; it does not
cover the runtime scheduler transition.
