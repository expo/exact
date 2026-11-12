# `tamber tool --via` forwards an import of up to 16 MiB; the protocol says its proxy stays at 1 MiB

**Status:** Closed
**Resolution:** The protocol now says the proxy sends on a token-bearing `import` up to 16 MiB and every other request up to 1 MiB, the proxy's 413 names the limit it applied, and the proxy's test covers the token-bearing import, propose and over-16 MiB cases.
**Systems:** scripts/tamber-server.mjs, docs/tamber-protocol.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`docs/tamber-protocol.md:256-259` gives the body limits of `tamber/1.17`:

> The local loopback token's body may be 16 MiB (a CLI import's sources, …); every paired, remote or relayed client's stays 1 MiB, as does a request through `tamber tool --via`'s proxy.

The proxy does otherwise. `forward` reads up to 16 MiB of a `POST /__tamber` that carries a token header, and sends it on when its method is `import` (`scripts/tamber-server.mjs:1446-1449` at `dd6f5dcd`):

```js
const limit = req.method === 'POST' && url.pathname === '/__tamber' && req.headers['x-exact-tamber-token'] ? LOCAL_BODY_LIMIT : BODY_LIMIT;
const bytes = await readBody(req, limit, res, true);
if (bytes == null) { if (bytes === null && !res.destroyed) json(res, 413, { error: { message: 'a request is at most 1 MiB' } }); return true; }
if (bytes.length > BODY_LIMIT && (url.pathname !== '/__tamber' || parse(bytes.toString('utf8'))?.method !== 'import')) { … 413 … }
```

The dev loop then judges the token itself, so the local token's import reaches it through the proxy as it would directly. Commit `4756891b` added these lines and left the protocol's sentence as it was; the sentence is the part that is wrong.

One message is wrong too. A body over 16 MiB on that path is answered "a request is at most 1 MiB" (the third line above), where the server's own answer to the same request is "a request is at most 16 MiB".

## The failure

`POST /__tamber` through `forward`, to a stand-in loop that counts the bytes that arrive:

```
import, 2 MiB, no token header:   413 {"error":{"message":"a request is at most 1 MiB"}}
import, 2 MiB, a token header:    200 {"ok":{"bytes":2097214}}
import, 15 MiB, a token header:   200 {"ok":{"bytes":15728702}}
import, 17 MiB, a token header:   413 {"error":{"message":"a request is at most 1 MiB"}}
propose, 2 MiB, a token header:   413 {"error":{"message":"a request is at most 1 MiB"}}
bodies the loop received (bytes): [ 2097214, 15728702 ]
```

Expected by the protocol's sentence: the second and third lines refused as the first is.

## Verification

Ran those five requests at `dd6f5dcd` against `forward(req, res, url, { local: true }, <the stand-in's URL>)`; the output is above. The proxy's test (`scripts/tamber-body-limit.test.mjs:105`) sends bodies with no token header, so the 16 MiB path has no test. Not run: a `tamber import` of a source over 1 MiB through a running `tamber tool --via`.

## Direction

Correct the protocol's sentence (the proxy passes an import that carries a token up to 16 MiB, every other request 1 MiB), say "16 MiB" in the proxy's 413 when that was the limit applied, and add the token-bearing import to the proxy's test.

## Related

No issue or QUEUE entry mentions the proxy's body limit.
