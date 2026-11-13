# A streamed fetch drops a Request body when init.body is absent

**Status:** Open
**Systems:** host/web-js
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`request` copies bytes only from `init.body`. It takes a `Request`'s url, method, headers, cache, and signal. It rejects a long poll when `request.body` is set and `init.body` is null, and otherwise leaves `raw` empty. Plain `fetch` passes that `Request` to the browser, so the body is sent. An `init.body` that is a stream is refused. This path sends nothing.

`fetch(new Request(url, { method: 'POST', body }), { exactStream })` or the same with `exactUploadProgress` delivers an empty body. The upload can end as load with no bytes, and those arguments are then marked sent.

## Where

`host/web-js/ts-stream.js`, `request`. `host/web-js/ts-body-from.js`, the long-poll check that already sees `request.body`.

## How to reproduce

Call `fetch(new Request(url, { method: 'POST', body: 'hello' }), { exactStream })`. The request that leaves has no body. The same `Request` without `exactStream` sends `hello`.

## Direction

If `request.body` is non-null and `init.body` is null, throw the stream-body refusal before open. Do not send an empty `raw`.
