# Recipe: a Convex backend (live queries, mutations)

`@exact/convex` keeps Convex queries live over one socket: a write anyone makes
shows in every query it changes, in one commit, with nothing polled. Install:
`EXACT_APP_DIR=$PWD bun <exact2>/scripts/exact.mjs add convex`, `bun add convex`,
then `npx convex dev`, Convex's own. The rest (errors, tokens, tests, the HTTP
client): `bun exact.mjs docs convex live queries`. A whole app: `apps/mosaic`.

```ts
import { convex, ConvexFailure } from '@exact/convex';
import { getFunctionName } from 'convex/server';
import { convexToJson, jsonToConvex } from 'convex/values';
import { api } from './convex/_generated/api';

// The deployment's socket origin: the grant and the client both take it.
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
  // A closed socket is opened again after 1, 2, 4, 8, then every 16 seconds.
  // The host closes one that carries nothing for a minute.
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
