# Recipe: authenticate a WebSocket with a single-use ticket

The [fixture app](socket-ticket/app.ts) authenticates its ticket request, then
opens the socket with `?ticket=`. Its [Contract](socket-ticket/app.contract),
[test server](socket-ticket/server.mjs), and [reconnect test](socket-ticket/app.test.contract)
are runnable code. The server requires authorization, expires tickets after one
minute, and consumes each ticket before upgrading. A reconnect requests a new ticket.

Use your HTTPS ticket endpoint and WSS socket. In `app.ts`, replace the fixture's
`authorizedFetch` with `(store, url, options) => provider.fetch(store, url, options)`,
or another authorized HTTPS fetch. The `ticket` source receives `store` as its second
argument and passes it to that call. `provider.fetch` keeps its approved HTTPS destinations and Bearer-header
behavior (LLP 1146 D5); the socket itself uses ordinary `fetch` with `exactStream`,
under a separate `net.websocket` grant. For a service that authenticates in the first
socket message, `provider.accessToken(store)` exposes the held token to data code; see
[OAuth tokens for sockets](../contract-for-agents.md#sign-in-providers-exactoauth).
It does not authenticate the upgrade.

The two sources are intentional (LLP 1016.000): `ticket` awaits the HTTPS response,
keeps the ticket in data code, and returns only an attempt number. That number asks
`events`, which consumes the stored ticket and opens its stream before its first
`await`. Opening the stream after awaiting the ticket in the same source is refused
on the web. Reconnect through the button's action, which changes the attempt and
requests another ticket; refreshing only the socket source cannot reuse one. Starting
an attempt clears the previous ticket, and a superseded response retains no ticket.

The example offers the protocol name `events.v1` and displays `event.protocol`.
Do not put access tokens in protocol names: their alphabets differ, and the server
echoes the selected protocol. Tickets also remain credentials until consumed or
expired: keep them out of Contract answers and redact their query from server,
proxy and diagnostic logs. A production issuer binds each ticket to its authenticated
user and intended socket; redeem it atomically in the server's shared store.

Run the fixture from the checkout root:

```sh
bun test docs/recipes/socket-ticket/recipe.test.mjs
SOCKET_HOST=macos bun test docs/recipes/socket-ticket/recipe.test.mjs
```

The tests start an isolated loopback HTTP/WS server with a fixture-only identity.
They do not contact an identity provider. To try the app by hand:

```sh
bun docs/recipes/socket-ticket/server.mjs
cd docs/recipes/socket-ticket
SOCKET_API=http://127.0.0.1:4798 SOCKET_URL=ws://127.0.0.1:4798 bun exact.mjs web
# Or: SOCKET_API=http://127.0.0.1:4798 SOCKET_URL=ws://127.0.0.1:4798 bun exact.mjs mac --run
```

Loopback's cleartext transport is only for this test fixture. `SOCKET_API` and `SOCKET_URL` default to HTTPS and WSS placeholders; set both
to your real endpoints when adapting the recipe.
