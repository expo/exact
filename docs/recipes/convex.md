# Recipe: a Convex backend (live queries, mutations)

`@exact/convex` updates live queries in one commit over one socket. Install
`EXACT_APP_DIR=$PWD bun <exact2>/scripts/exact.mjs add convex`, `bun add convex`,
then `npx convex dev`. See `bun exact.mjs docs convex live queries` for errors,
tokens, tests and the HTTP client.

```ts
import { convex, ConvexFailure } from '@exact/convex';
import { getFunctionName } from 'convex/server';
import { convexToJson, jsonToConvex } from 'convex/values';
import { api } from './convex/_generated/api';

// The grant and client share the socket origin.
const CONVEX = 'wss://happy-otter-123.convex.cloud';
export const grants = `net.websocket ${CONVEX}`;

const cx = convex(CONVEX, { getFunctionName, convexToJson, jsonToConvex });
const sources: Sources = {
  live: () => cx.live(),
  messages: ([channel, rev]) => cx.query(rev, api.messages.list, { channel }),
  send: ([channel, body]) => cx.mutation(api.messages.send, { channel, body }).then(
    () => ({ ok: true, error: '' }),
    (e) => ({ ok: false, error: e instanceof ConvexFailure ? e.message : 'Try again.' })),
};
```

```text
use ConvexLive from "@exact/convex"   // at the top of the file

component Chat
  resource live = live() as shape ConvexLive else empty(status="connecting")
  resource messages = messages(channel, live.rev) as shape Message[] else empty()
  mutation sent as shape Reply then afterSend
  state waited = 0
  state wait = 1
  task reconnect when live.status == "closed"
    every(1000, retry)
  task recovered when live.status == "ready" and wait > 1
    after(1000, calm)
  action retry
    if waited + 1 >= wait
      waited = 0
      wait = min(wait * 2, 16)
      refresh live
    else
      waited = waited + 1
  action calm
    wait = 1
    waited = 0
```

- **One `live` resource, at the root:** the socket. Show `live.status`:
  `connecting`, `open`, `ready`, `closed` (`retry` refreshes it) or `failed`.
- **Every query takes `live.rev`,** first, in `cx.query`. When a result
  changes, `rev` rises and each query answers again at once.
- **No `refreshes` after a write:** a mutation's reply arrives once the queries
  show what it wrote.
- **Shape a result in the same call,** `cx.query`'s fourth argument: `(b) => ({
  tiles: b.cells.map(…) })`. An `async` wrapper makes every change a pending
  frame.
- **`cx.setAuth(jwt)`** in a mutation's source, after any `fetch` it awaits:
  a source that does both before its first `await` is refused.
- **A drive against a real deployment** takes a new `--seed` each
  launch, or Convex answers the second launch's writes without running them.

Use `apps/mosaic/server.ts` as a model for tests. The stand-in handles
`POST /api/query`, `/api/mutation` and `/api/action` before its `http` callback.

## Convex Auth v2

Call the deployment's mutations and use `openAuthSession` for the browser step.
State in `redirectTo`, `convexAuthCode`, resuming, renewing and signing out:
`bun exact.mjs docs convex auth`. Native setup:
[HTTPS callbacks](../contract-for-agents.md#https-callbacks).
