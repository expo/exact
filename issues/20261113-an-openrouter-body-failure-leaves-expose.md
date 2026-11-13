# An OpenRouter body failure leaves Expose stuck answering

**Status:** Open
**Systems:** apps/expose
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`ask` sets `streaming` before the request. The rejection handler on `fetch` covers only the request itself failing. `response.json()`, `response.text()`, and `reader.read()` run inside the success callback. A throw there rejects the promise `then` returns, and nothing calls `finish`.

`streaming` stays true, the bubble stays in progress, and `clearThread` returns without clearing while that flag is set. Every later send hits the still-answering return and drops the draft. The thread accepts nothing until the process restarts.

A non-JSON 200 and a stream that errors after the headers are ordinary failures from that endpoint.

## Where

`apps/expose/app.ts`: `ask` sets `streaming` and returns `fetch(...).then(async …, onRejected)`. `clearThread` returns immediately when `streaming` is set.

## How to reproduce

Point the request at an endpoint that returns a 200 whose body is not JSON (the native, non-stream path calls `response.json()`), or that errors the body after the headers (the page path's `reader.read()`). The bubble never settles and Clear does nothing.

## Direction

Call `finish` on every exit, including a throw, and let Clear abort the in-flight read so the turn can end.
