surfaces: web
agent: Codex (GPT-5)
task: verify sign-out through the browser wasm host

### Rough

- The shared web launcher carried the test server as `?authFixture=`, but the wasm auth glue did not pass it to the runner. The driver then refused the redacted authorization URL. Forwarding that existing field made the authored drive work.
- `docs/diary.md` says to run `bun exact.mjs feedback status` from an app, but an in-repository app has no `exact.mjs`; the command failed with `Module not found "exact.mjs"`. This diary is the repository record instead.

### Lean in

- One authored drive started the fixture server, served the wasm build, completed the held authorization in headless Chrome and checked revocation.

### Needed

- sign-in: provided

### Checkpoints

- install / first build: smooth
- editing app.contract / app.ts: smooth
- driving the app: rough (auth fixture carrier; see Rough)
