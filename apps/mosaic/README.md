# Mosaic

One board of tiles that everyone paints together, live, on [Convex](https://convex.dev)
through `@exact/convex` (LLP 1157). A stroke anyone paints shows on every
screen at once, in the board and in the list of latest strokes together.
Nothing polls: the app holds one socket.

```sh
bun install
npx convex dev            # Convex's local deployment at http://127.0.0.1:3210 (no account needed:
                          #   CONVEX_AGENT_MODE=anonymous npx convex dev)
bun exact.mjs web         # the app, in a browser; open it twice
bun exact.mjs mac --run   # and as a Mac app beside it (ios --run: on a simulator)
```

- `app.contract`: the view, and the tasks that open a closed socket again.
- `app.ts`: three lines of sources over `@exact/convex`.
- `convex/board.ts`: the Convex functions; `convex/tiles.ts`, the rules they run.
- `server.ts`: the tests' stand-in for the deployment, over the same rules.
  `bun exact.mjs test` and `verify` start it, so they need no Convex. Stop
  `npx convex dev` first: both listen on 3210.

A cloud deployment is named once in `app.ts`: `CONVEX`, its socket's origin
(`wss://<name>.convex.cloud`), which the client and the `net.websocket` grant
both take.
