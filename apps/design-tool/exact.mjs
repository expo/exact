// `bun exact.mjs <verb>` for the design tool, as an app `exact new` made has
// it (scripts/app-cli.mjs `run`): `bun exact.mjs test` runs its contract
// tests against the stub on this checkout's own port, never 7740
// (scripts/tamber-tool-port.mjs); `bun exact.mjs help` lists the verbs.
import { resolve } from 'node:path';

const EXACT = resolve(import.meta.dir, '../..');
const { run } = await import(resolve(EXACT, 'scripts/app-cli.mjs'));
process.exit(await run({ dir: import.meta.dir, name: 'design-tool', exact2: EXACT, args: process.argv.slice(2) }));
