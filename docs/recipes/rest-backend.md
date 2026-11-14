# Recipe: your own REST server

The REST starter creates a shared-list app and a Bun server:

```sh
bun exact.mjs pattern rest
```

The [REST starter](../patterns/rest/app.contract) writes the Contract app and
Bun server together. Its data source validates read responses before returning
them. When a read fails because of a transport error, an HTTP error or an invalid
response, Exact keeps the prior answer and exposes the failure. The view shows
that answer only when `lib.owner == persona`; otherwise, it shows loading or
retry state. A validated empty list is a successful answer.

The starter's writes return shaped replies for operation refusals, such as a
duplicate title, so `then` can keep the form open and display the reason. Its
mutation helper also converts transport failures to shaped replies. If a source
lets a transport rejection escape instead, the mutation gets no new reply, keeps
its previous value, and does not run `then`; `failure(mutation)` explains the
request failure.

The sample `persona` values are public development identities. A real app should
put a stable, non-secret principal id in the resource call and keep bearer
credentials in the granted store. It should await an app-owned current-session
helper inside the data source before issuing authenticated requests, then verify
that the session authorizes the principal named by the resource. Exact does not
provide that session helper. This starter demonstrates owner gating, not a full
authentication lifecycle; a real authenticated view must also gate retained
answers by the app-owned authorized principal and auth generation.

## Where the server lives

`test`, `verify` and `agent --test` start the server themselves, empty for each test,
and stop it after; start none of your own for them. They find it this way:

- **Its entry:** `server.ts`, `server/index.ts` or `server/server.ts`, or the one `app.json` names:
  `"commands": { "server": ["bun", "server/main.ts"] }`. Another program runs as written
  (`["python3", "server.py"]`), told `DB` and `PORT` as Bun's is; its first argument that is a file
  of the app is the entry checked for `DB`. Arguments after the entry are passed through in order.
- **Its address:** the first `http://127.0.0.1:<port>` in `app.ts` as the build has it, a setting's
  value in place of its read (or in a module it imports, as the starter's `API`). To use another port, change it there; the server reads it from the same constant.
  An origin fixed at build (a deployed API's) is a setting: `"env": ["API_BASE_URL"]` in `app.json`,
  `const API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8080'` in `app.ts`, its value from the
  environment, `.env.local` or `.env`, in that order. Keep the local server's address as the default.
  The value is inlined into the module on every host, so a secret does not belong there.
- **Its database:** the path in `DB`, which the entry or a module it imports reads:
  `new Database(process.env.DB ?? 'data.db')`.
  If the server keeps no database, set `"server": { "database": false }` beside
  `commands` to omit `DB` and skip scratch database creation and removal; otherwise
  an entry that does not read `DB` is refused.

Each test starts with empty client storage and an empty server: between tests the process
restarts and its scratch database is removed. `database: false` restarts too, clearing memory.
`reload` and `request` within a test keep its data. `verify` reruns its chosen test empty before
relaunching on the store it leaves, with that server still running.
Remote servers stay shared. Only `test` and `agent --test` share an ancestor's server
(an older `api-test` wrapper's), saying so; `verify` refuses an answering local server,
including an ancestor's.
Snapback’s `fresh` remains opt-in.

A dev server already on that port makes them stop and name its pid: stop it first.
