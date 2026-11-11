// `bun exact.mjs <verb>` for Mosaic, as an app `exact new` made has it
// (scripts/app-cli.mjs `run`): `bun exact.mjs test` runs its contract tests
// against server.ts, `bun exact.mjs help` lists the verbs.
import { resolve } from 'node:path';

const EXACT = resolve(import.meta.dir, '../..');
const { run } = await import(resolve(EXACT, 'scripts/app-cli.mjs'));
process.exit(await run({ dir: import.meta.dir, name: 'mosaic', exact2: EXACT, args: process.argv.slice(2) }));
