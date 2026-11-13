surfaces: web
agent: Codex (GPT-6), Claude Opus 5.5
task: Clear held requests when leaving a wave.

## Rough

The fixture's static handler returned 404 for the JS build's lazy agent chunk.
The separately served build cleared pending lanes on navigation but left HTTP
reads held at the fixture. Both blocked the documented smoke.

The first fix put the cancellation policy in the web runtime's shared code,
which every web app ships, and pushed the production starter over its 45 KiB
budget. The policy (which requests are reads) now lives in `rust-data.js`,
which only apps with a Rust data module load.

## Lean in

The existing agent exposed pending lanes, request abandonment and stale-reply
rejection without adding instrumentation to the app.

## Needed

- HTTP cancellation: provided for ordinary Rust reads on the JS target.

## Checkpoints

- Web smokes passed with 1 and 128 lanes through the fixture, a separate web
  server and the agent's own server. Each cleared held work, settled the clock,
  dropped old replies and completed the remounted wave.
- Browser regression tests cover cancellation after a successful commit, a
  refused navigation that keeps its reads, and late replies after remount.
  Ordinary reads stay out of the agent's stream list.
- Web smokes passed with 1 and 128 lanes on the wasm target.
