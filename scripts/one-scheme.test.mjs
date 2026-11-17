// A build's text says the colours that stay one in dark mode in one line, past one (LLP 1168.000 D7.2): a drive's
// lines, an Apple bake's, verify's and contract's own `one_scheme_line` agree; the file a drive writes lists each.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { oneScheme, warningLines } from './app-cli.mjs';
import { bakeWarnings } from './linked-plan.mjs';
import { report } from './verify.mjs';
import { acceptModuleReply } from '../host/web/dev-phase.mjs';

const site = (line, colour) => `app.contract:${line}:9 warning [lower-colour-one-scheme] \`${colour}\` on \`color\` stays the same in dark mode: name a role (\`-exact-label\`) or leave it unsaid`;
const sites = [site(4, '#111111'), site(5, '#222222'), site(7, '#333333'), site(9, '#444444')];
const close = 'app.contract:4:9 warning [lower-colour-one-scheme] these 4 literal colours stay the same in dark mode while the platform\'s text and controls change: name a role for each or leave it unsaid. Leave the accent unsaid, or write it as a role or one `light-dark(light, dark)` pair on `accent-color`.';
const token = 'app.contract:2:3 warning [lower-colour-one-scheme] `ink` is the same colour in light and dark (`#000000`): give it a `light-dark(light, dark)` pair, or mark it `scheme=fixed`';
const header = 'app.contract:12:3 warning [lower-header-unplaced] this `button` in this route\'s `header` is not shown on iOS';
const one = 'app.contract:4:9 warning [lower-colour-one-scheme] these 4 literal colours (app.contract:4, app.contract:5, app.contract:7, …) stay the same in dark mode while the platform\'s text and controls change: name a role for each or leave it unsaid. Leave the accent unsaid, or write it as a role or one `light-dark(light, dark)` pair on `accent-color`. `contract build --json` names each.';

test('several literal colours are one line, their closing line naming the first three sites, where the first stood', () => {
  assert.deepEqual(oneScheme([header, ...sites, close]), [header, one]);
  // A theme token's line says its own repair and stays; one site keeps its own line whole.
  // A theme token is a site too, named among the three and counted; its repair is --json's to say.
  const mixed = 'app.contract:2:3 warning [lower-colour-one-scheme] these 5 colours stay the same in dark mode while the platform\'s text and controls change: name a role for each literal (each token\'s line says its own repair) or leave it unsaid.';
  assert.deepEqual(oneScheme([token, ...sites, mixed]), ['app.contract:2:3 warning [lower-colour-one-scheme] these 5 colours (app.contract:2, app.contract:4, app.contract:5, …) stay the same in dark mode while the platform\'s text and controls change: name a role for each literal (`contract build --json` says each token\'s repair) or leave it unsaid.']);
  const tokens = 'app.contract:2:3 warning [lower-colour-one-scheme] these 2 tokens are the same colour in light and dark: each line above says how to repair it';
  assert.deepEqual(oneScheme([token, token.replace(':2:3', ':3:3').replace('`ink`', '`paper`'), tokens]), ['app.contract:2:3 warning [lower-colour-one-scheme] these 2 tokens (app.contract:2, app.contract:3) are the same colour in light and dark: `contract build --json` says how to repair each']);
  assert.deepEqual(oneScheme([header, sites[0]]), [header, sites[0]]);
  // Three or fewer are all named, with no ellipsis.
  assert.match(oneScheme([sites[0], sites[1], close])[0], /literal colours \(app\.contract:4, app\.contract:5\) stay/);
});

test('a drive and an Apple bake say the one line; the drive\'s file lists every site', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-one-scheme-')));
  try {
    const said = warningLines(dir, 'web', [header, ...sites, close]);
    assert.deepEqual(said.slice(0, 2), [header, one]);
    assert.match(said[2], /^compiler: 6 warnings; /);
    assert.equal(readFileSync(resolve(dir, '.exact', 'warnings-web.txt'), 'utf8'), [header, ...sites, close].map((w) => `${w}\n`).join(''));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  assert.deepEqual(bakeWarnings('chat', [header, ...sites, close]), [`warning: chat: ${header}`, `warning: chat: ${one}`]);
  // A drive's bake keeps them whole, so its file lists each.
  process.env.EXACT_WARNINGS_WHOLE = '1';
  try { assert.equal(bakeWarnings('chat', [header, ...sites, close]).length, 6); } finally { delete process.env.EXACT_WARNINGS_WHOLE; }
});

test('a drive says the one line again when a colour changes at the same sites', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-one-scheme-again-')));
  try {
    warningLines(dir, 'web', [header, ...sites, close]);
    assert.deepEqual(warningLines(dir, 'web', [header, ...sites, close]).length, 1, 'unchanged: the count alone');
    const changed = [header, sites[0].replace('#111111', '#555555'), ...sites.slice(1), close];
    const said = warningLines(dir, 'web', changed);
    assert.deepEqual(said.slice(0, -1), [one]);
    assert.match(said.at(-1), /^compiler: 6 warnings, 1 new \(above\); /);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('verify\'s report says the colours in one line and counts each', () => {
  const { lines } = report({ host: 'web', built: null, compiler: [header, ...sites, close], tested: { passed: 1, failed: 0, results: [] }, checks: [], shots: [], ms: 1000, cwd: '/app' });
  const said = lines.join('\n');
  assert.equal(said.split(' warning [lower-colour-one-scheme] ').length - 1, 1, said);
  assert.ok(said.includes('these 4 literal colours (app.contract:4, app.contract:5, app.contract:7, …)'), said);
  assert.match(said, /^compiler: 6 warnings$/m);
});

test('the web dev loop says the colours in one line', () => {
  const logged = [];
  acceptModuleReply(JSON.stringify({ id: 1, ok: true, warnings: [header, ...sites, close] }), { id: 1 }, 1, (w) => logged.push(w));
  assert.deepEqual(logged, [header, one]);
});
