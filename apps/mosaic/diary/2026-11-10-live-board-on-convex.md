surfaces: web, macos, ios
agent: Claude Code (Fable 5.2)
task: a shared board that updates live from a hosted backend's subscriptions, written with the module the same change adds

### Rough

- A standard module that imports an npm package the app also imports does not build: `two copies of convex@1.49.0`. The bake resolves a linked package's imports from the package's real place, so `packages/convex` found the checkout's copy and the app's generated `api` found the app's. About 20 minutes; `@exact/convex` now takes Convex's three pure functions as arguments and imports nothing (QUEUE.md has the line).
- A package's `exports` `types` must be a `.d.ts`: `"types": "./index.ts"` gave `Cannot find module '@exact/convex'` with no word about why. `@exact/oauth`'s pair (`index.d.ts`, and a test that the two agree) was the thing to copy.
- `grants` cannot be computed from the deployment's URL (`CONVEX.replace(/^http/, 'ws')` is refused: "grants must be a constant"), so the app names the deployment twice, as `https:` and as `wss:`. A grant that took the `https:` origin for a socket, or a build-time setting in a grant, would make it one line.
- `expect state painted == some(Reply(…))` is refused; `expect state painted.ok == true` reads through the option. The message lists what the right side may be and not that the left may path into an option.
- A test's `tap` on a disabled button fails the test. Clear was disabled on an empty board, and the tests clear first, so Clear is always enabled.
- The board was twelve tiles across. `verify ios` reported 153 touch targets under 44 pt (29 pt tiles, 34 pt swatches), which no web check had: eight across and six hues fit a phone. About 10 minutes; the finding named the size and the fix.
- `bun exact.mjs ios` stops when several simulators are booted and none has the app, and names them; `--sim <name>` chose one.
- One server serves a whole test run, so tests that count strokes clear the board first. "Each from empty storage" is the app's storage, not its server's.
- A task's interval is a literal, so a backoff of 1, 2, 4, 8, 16 s is a one-second task that counts. It works and reads oddly.
- On macOS a second drive against the real backend painted nothing while every mutation replied ok. About 25 minutes, with frames logged, to see why: the driver seeds a native host's `crypto.getRandomValues` (seed 1 unless `--seed` says another), the module draws its session id from it, and Convex answers a request id it has seen from a session with the first answer. A new `--seed` each launch fixes the drive. Nothing in `state` or the journal says the randomness is seeded; a line at launch would have.
- `clock +20000 real` was the way to watch a pushed change and a reconnect arrive; it took a `--help all` to find.
- The quiet-socket watch ran only while `live.status == "ready"`, so a socket that opened and carried nothing was never opened again. A reviewer found it, not a test: no test holds a server silent. It now runs for an open and an opening socket too, as one tick a minute.

### Lean in

- The runner's socket did everything the protocol needed with no change: a mapper that answers `open` with the query set, answers that send their own subscription, a mutation that awaits its reply. The first drive against a real backend worked.
- A changed argument asking again, with an answer given now landing in the same commit, is what makes two queries change together on screen.
- `commands.server` starting `server.ts` for `test` and `verify`: the app's tests are hermetic with fifteen lines.
- Platform colour roles as a palette: the board follows dark mode with nothing written.
- `verify` found the one clipped label at the largest text size and said what to change.

### Needed

- live server data over a socket: provided (the stream's socket, LLP 1016.000 D7), with `@exact/convex` by hand in the same change
- reconnecting a dropped socket: by hand (~10 lines of Contract tasks; the runner leaves it to the app)
- a persisted setting: provided (`persist`)

### Checkpoints

- 2026-11-10: on the web against Convex's local backend (`npx convex dev`, the open-source backend, no account): the board and the latest strokes arrive over one socket; a stroke lands in both in one commit; a write from outside the app (an HTTP mutation) shows with nothing polled; with the backend stopped the app says so, refuses a stroke without sending it, and reconnects by itself when the backend returns, the board intact.
- 2026-11-10: `bun exact.mjs verify` is clean on the web against `server.ts` (4 tests; light and dark at two widths; the largest text size).
- 2026-11-10: the Mac app (`bun exact.mjs mac`) against the same backend, with a new `--seed`: the board arrives, a stroke lands in both queries, a write from outside shows with nothing polled. `bun exact.mjs test macos` passes its 4 tests against `server.ts`.
- 2026-11-10: on an iOS simulator, `bun exact.mjs verify ios` is clean against `server.ts`. The real backend was not driven from the simulator.
- 2026-11-10: on the web against `server.ts`, `agent web "clock +130000"` with the clock jumped past two ticks and no frame between them: `logs` shows the stream let go and the socket opened again, and the board is `ready`. The 4 tests pass.
- 2026-11-11: the watch task and `live.beat` are gone: the module asks its socket with `exactIdleTimeout` and the host ends a silent one. The Mac app against a peer that accepts the socket and never sends: `agent macos 'clock until state live.status == "closed" within 90000'` met after 60,011 ms of real time with the driver's clock at 0, `live.error` "nothing arrived for 60000 ms", and the peer saw the close at 60.0 s. The 4 tests pass on the web.
- 2026-11-12: the deployment is named once: `CONVEX` is the socket's origin, which `convex()` takes and the grant is a template over. The test tooling found no server to start from a `ws:` origin (it looked for an `http:` one), which `scripts/verify.mjs` now reads as the server's. The 4 tests pass on the web.
