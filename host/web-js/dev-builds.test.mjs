// The JS dev loop's builds (host/web-js/dev.mjs): a build that an input
// moved during is set aside, never published (not dist, which a fresh page
// loads, nor the design or a reload); an edit stops a young build; a value
// batch during a verifying build applies at once, and the build after
// carries it. EXACT_DEV_BUILD_HOLD_MS holds each build's end, as a slow one.
import { afterEach, expect, test } from 'bun:test';
import { spawnOwned, stopAll } from '../../scripts/test-children.mjs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { INPUT_TREE, OUTPUT, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { shaderWatchRoots } from '../../scripts/app.mjs';
import { within } from '../../scripts/paths.mjs';
import { runInNewContext } from 'node:vm';
import { installLock } from '../../scripts/filesystem.mjs';
import { appSources } from '../web/serve.mjs';
import { loopWebDist } from '../../scripts/web-build.mjs';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const sha = text => createHash('sha256').update(text).digest('hex');

// A dev loop a test started ends with the test, a timeout included.
afterEach(() => stopAll());

/** The JS dev loop on the app at `dir`, its dist in a temporary directory,
 * `env` beside the process's own. */
async function devLoop(dir, env = {}) {
  const listener = createServer();
  await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port;
  await new Promise(ok => listener.close(ok));
  // The build's lock and kept generations go beside its dist (serve.mjs `replaceBuild`): the dist sits in a directory of its own.
  const dist = env.EXACT_WEB_DIST === '' ? loopWebDist({ dir, target: resolve(dir, 'target') }, 'js') : join(mkdtempSync(join(tmpdir(), 'exact-js-builds-')), 'dist');
  // Owned: ended with what it starts when the test ends, a timeout included (scripts/test-children.mjs).
  const dev = spawnOwned(process.execPath, [resolve(ROOT, 'host/web/dev.mjs'), '--app', dir, '--port', String(port)], { cwd: ROOT, env: { ...process.env, EXACT_WEB_DIST: dist, ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
  const stop = async () => {
    await dev.stop(30_000);
    rmSync(dirname(dist), { recursive: true, force: true });
  };
  let out = '', err = '';
  dev.stderr.on('data', d => { err = (err + d).slice(-4000); });
  try {
    await new Promise((ok, fail) => {
      dev.stdout.on('data', d => { out += d; if (out.includes(`127.0.0.1:${port}/`)) ok(); });
      dev.on('error', fail);
      dev.on('exit', code => fail(new Error(`the dev loop exited ${code}: ${out}${err}`)));
    });
  } catch (e) { await stop(); throw e; }
  return { base: `http://127.0.0.1:${port}`, stop, said: () => out, dist };
}

test('the JS loop carries a checkpoint literally across document probes and retries', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-checkpoint-dollars-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Checkpoint', app: { id: 'test.checkpoint', name: 'Checkpoint' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  state body = "initial"\n  view\n    text body testId="body"\n');
  const body = "$` $' $& $$ $5 $HOME </script><script>globalThis.__injected=true</script>";
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base);
    await page.waitForFunction(() => globalThis.exact?.ready);
    const checkpoint = await page.evaluate(() => ({ time: 0, carryAnswers: false, answers: [],
      slots: globalThis.exact.state[0].map(s => [s.n.devName, s.n.devType, s()]),
    }));
    checkpoint.slots.find(([name]) => name === 'body')[2] = body;
    const post = () => fetch(`${loop.base}/__dev/checkpoint?id=1-2-3-4&seq=1`, {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(checkpoint),
    });
    expect((await post()).status).toBe(204);
    expect((await fetch(`${loop.base}/?__exactDev=1-2-3-4`, { method: 'HEAD' })).status).toBe(200);
    const abandoned = await fetch(`${loop.base}/?__exactDev=1-2-3-4`);
    await abandoned.body.cancel();
    const html = await (await fetch(`${loop.base}/?__exactDev=1-2-3-4`)).text();
    const data = html.match(/<script type="application\/vnd\.exact\.dev-checkpoint"[^>]*>(.*?)<\/script>/s)?.[1];
    expect(data).toBeDefined();
    expect(JSON.parse(data)).toEqual(checkpoint);
    expect(html.match(/<!doctype html>/gi)).toHaveLength(1);
    await page.goto(`${loop.base}/?__exactDev=1-2-3-4`);
    await page.waitForFunction(() => globalThis.exact?.ready);
    expect(await page.getByTestId('body').textContent()).toBe(body);
    expect(await page.evaluate(() => globalThis.__injected)).toBeUndefined();
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test.each([false, true])('a blurred autofocus field stays blurred after a carried reload (bound: %s)', async bound => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-blurred-carry-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Blurred carry', app: { id: 'test.blurred.carry', name: 'Blurred carry' } }));
  const contract = label => `component App
  state wantsFocus = true
  view
    column
      input autofocus=${bound ? 'wantsFocus' : 'true'} testId="draft"
      text "${label}" testId="label"
`;
  writeFileSync(join(dir, 'app.contract'), contract('before'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    expect(loop.said()).not.toContain('does not build');
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base);
    await page.waitForFunction(() => document.activeElement === document.querySelector('[data-testid="draft"]'));
    await page.getByTestId('draft').blur();
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
    writeFileSync(join(dir, 'app.contract'), contract('after'));
    await page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === 'after');
    // Let the browser flush its own autofocus candidates as well as the runtime's mount work.
    await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test('the JS loop drops settled answers when a page module or its imported helper changes', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-native-carry-'));
  const modules = join(dir, 'modules/web');
  mkdirSync(modules, { recursive: true });
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Native carry', app: { id: 'test.native.carry', name: 'Native carry' } }));
  const contract = label => `shape Sample
  value: string
component App
  state draft = "initial"
  resource sample = sample() as shape Sample
  action edit(value)
    draft = value
  view
    column
      input value=draft input=edit testId="draft"
      text sample.value testId="sample"
      text "${label}" testId="label"
`;
  writeFileSync(join(dir, 'app.contract'), contract('before'));
  writeFileSync(join(dir, 'app.ts'), `import type { Answer } from './app.contract.d.ts';
export const grants = '';
export const answer: Answer = (_source, _args, _store, _storage, native) => native!.later({}) as ReturnType<Answer>;
`);
  const helper = value => `export const value = () => '${value}:' + crypto.getRandomValues(new Uint32Array(2)).join('.');\n`;
  const entry = suffix => `import { value } from './value.js';\nexport const later = () => ({ value: value() + '${suffix}' });\n`;
  writeFileSync(join(modules, 'value.js'), helper('before'));
  writeFileSync(join(modules, 'index.js'), entry('entry-before'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    expect(loop.said()).not.toContain('does not build');
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base);
    await page.waitForFunction(() => document.querySelector('[data-testid="sample"]')?.textContent.startsWith('before:'));
    const before = await page.getByTestId('sample').textContent();
    const revision = () => readFileSync(join(loop.dist, '.exact-dev-logic.json'), 'utf8');
    const originalRevision = revision();
    await page.getByTestId('draft').fill('carried');
    writeFileSync(join(dir, 'app.contract'), contract('presentation'));
    await page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === 'presentation');
    expect(revision()).toBe(originalRevision);
    expect(await page.getByTestId('draft').inputValue()).toBe('carried');
    expect(await page.getByTestId('sample').textContent()).toBe(before);
    writeFileSync(join(modules, 'value.js'), helper('helper-after'));
    await page.waitForFunction(() => document.querySelector('[data-testid="sample"]')?.textContent.startsWith('helper-after:'));
    expect(revision()).not.toBe(originalRevision);
    const helperRevision = revision();
    writeFileSync(join(modules, 'index.js'), entry('entry-after'));
    await page.waitForFunction(() => document.querySelector('[data-testid="sample"]')?.textContent.endsWith('entry-after'));
    expect(revision()).not.toBe(helperRevision);
    expect(await page.getByTestId('draft').inputValue()).toBe('carried');
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test('the JS loop: a build an edit lands during is set aside, never published, and the newer files build', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-superseded-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Superseded', app: { id: 'com.exact.superseded', name: 'Superseded' } }));
  const view = (text) => `component App\n  view\n    column background-color="surface.card" min-height=40\n      text "${text}"\n`;
  writeFileSync(join(dir, 'app.contract'), view('qz-first'));
  writeFileSync(join(dir, 'app.forma'), 'forma superseded\n\ntheme\n  surface.card = light-dark(#f1f3f5, #16181c)\n');
  let loop = null;
  try {
    // Each build's end comes 2 s after it: an edit lands as one ends.
    loop = await devLoop(dir, { EXACT_DEV_BUILD_HOLD_MS: '2000' });
    const { said, dist } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    await until(() => said().includes('plan ready') && said().includes('design reader ready'), 900_000);
    // What dist holds, whatever file carries the text.
    const holds = (text) => { const walk = (d) => readdirSync(d, { withFileTypes: true }).some(e => e.isDirectory() ? walk(join(d, e.name)) : readFileSync(join(d, e.name)).includes(text)); return walk(dist); };
    expect(holds('qz-first')).toBe(true);
    // An edit, and another while its build runs: the first's build is set
    // aside, and only the second's is published.
    // Sampled throughout: a fresh page never loads the set-aside build.
    let stale = false;
    const sampler = setInterval(() => { try { stale ||= holds('qz-second'); } catch { /* replaced meanwhile */ } }, 10);
    writeFileSync(join(dir, 'app.contract'), view('qz-second'));
    // Its build (a fraction of a second here) has read the files, and is
    // held 2 s at its end.
    await new Promise(ok => setTimeout(ok, 1200));
    writeFileSync(join(dir, 'app.contract'), view('qz-third'));
    await until(() => said().includes('edit → plan ready') && holds('qz-third'), 300_000);
    await new Promise(ok => setTimeout(ok, 500));
    clearInterval(sampler);
    expect(said()).toContain('edit → a build superseded');
    // The input that moved, named (stage 15 D1.5).
    expect(said()).toMatch(/set aside: app\.contract moved while it built/);
    expect(stale).toBe(false);
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test('the JS loop: an edit stops a young build, leaving dist as it was; a value batch during a verifying build applies, and the next build carries it', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-stopped-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Stopped', app: { id: 'com.exact.stopped', name: 'Stopped' } }));
  const view = (text) => `component App\n  view\n    column background-color="surface.card" min-height=40\n      text "${text}"\n`;
  const design = (dark) => `forma stopped\n\ntheme\n  surface.card = light-dark(#f1f3f5, ${dark})\n`;
  writeFileSync(join(dir, 'app.contract'), view('qz-one'));
  writeFileSync(join(dir, 'app.forma'), design('#16181c'));
  let loop = null;
  try {
    loop = await devLoop(dir, { EXACT_DEV_BUILD_HOLD_MS: '4000' });
    const { said, dist, base } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 50)); } };
    const count = (re) => (said().match(re) ?? []).length;
    await until(() => said().includes('plan ready') && said().includes('design reader ready'), 900_000);
    const bytes = () => { const out = []; const walk = (d) => { for (const e of readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) e.isDirectory() ? walk(join(d, e.name)) : out.push(e.name, sha(readFileSync(join(d, e.name)))); }; walk(dist); return out.join(','); };
    let first = bytes(), seen = new Set();
    // A sample is one dist (the same directory before and after it is read;
    // a swap is two renames).
    const sample = () => { const ino = statSync(dist).ino, b = bytes(); return statSync(dist).ino === ino ? b : null; };
    const sampler = setInterval(() => { try { const b = sample(); if (b) seen.add(b); } catch { /* replaced meanwhile */ } }, 10);
    // Three edits at once: the first's build is stopped young, and only
    // that one (one stop in a burst); the last edit's build publishes.
    // The second edit must find the first's build begun: a build begins
    // after the edit's debounce and its own preparation (the compiler's
    // copy, the reader's build started), past 30 ms on a loaded machine,
    // and then both edits are one build, published, and the burst is
    // made again.
    let aside, plans;
    for (let tries = 0; ; tries++) {
      if (tries === 5) throw new Error(`no build of a burst's first edit was begun by its second, in 5 tries: ${said().slice(-2000)}`);
      first = bytes(); seen = new Set(); aside = count(/edit → a build superseded/g); plans = count(/edit → plan ready/g);
      const readers = count(/design reader ready/g);
      writeFileSync(join(dir, 'app.contract'), view(`qz-two-${tries}`));
      await new Promise(ok => setTimeout(ok, 30));
      writeFileSync(join(dir, 'app.contract'), view(`qz-three-${tries}`));
      await until(() => said().includes('(aborted)') || count(/edit → plan ready/g) > plans, 300_000);
      if (said().includes('(aborted)')) break;
      await until(() => count(/design reader ready/g) > readers, 300_000);
    }
    // The stopped build ends (set aside) and the next begins; an edit in
    // that one sets it aside too, but stops it not.
    await until(() => count(/edit → a build superseded/g) >= aside + 1, 60_000);
    await new Promise(ok => setTimeout(ok, 300));
    writeFileSync(join(dir, 'app.contract'), view('qz-four'));
    await until(() => count(/edit → plan ready/g) > plans, 300_000);
    expect(count(/\(aborted\)/g)).toBe(1);
    expect(count(/edit → a build superseded/g)).toBe(aside + 2);
    clearInterval(sampler);
    // dist was the first build's until the last one published: never the
    // stopped build's, nor a half-written one.
    const last = bytes();
    expect([...seen].every(b => b === first || b === last)).toBe(true);
    expect(last).not.toBe(first);
    const holds = (text) => { const walk = (d) => readdirSync(d, { withFileTypes: true }).some(e => e.isDirectory() ? walk(join(d, e.name)) : readFileSync(join(d, e.name)).includes(text)); return walk(dist); };
    expect(holds('qz-four')).toBe(true);
    // A value-only edit (its batch, then its verifying build, held at its
    // end) and another while that build runs: the second's batch applies
    // at once, the build is set aside, and the next carries the value.
    // (The reader's build is a cargo build on the shared target/, queued
    // behind its lock while another build holds it: a build's bound.)
    await until(() => count(/design reader ready/g) >= 3, 300_000);
    const supersededBefore = count(/edit → a build superseded/g);
    writeFileSync(join(dir, 'app.forma'), design('#202020'));
    await until(() => count(/edit → batch ready/g) >= 1, 60_000);
    await new Promise(ok => setTimeout(ok, 500));
    const held = Date.now();
    writeFileSync(join(dir, 'app.forma'), design('#303030'));
    // Its batch from the design read, not a build's, before the held
    // build could end.
    await until(() => count(/edit → batch ready .*no build/g) >= 2, 60_000);
    expect(Date.now() - held).toBeLessThan(3000);
    // Served at once (the pages' snapshot), before the held build ends.
    expect(JSON.stringify(await (await fetch(`${base}/__dev/design`)).json())).toContain('#303030');
    await until(() => count(/edit → a build superseded/g) > supersededBefore, 60_000);
    await until(() => said().includes('edit → checked'), 120_000);
    const served = await (await fetch(`${base}/__dev/design`)).json();
    expect(JSON.stringify(served)).toContain('#303030');
    // And during a full build (app.json's, which leaves the design's shape):
    // the batch applies at once, and the build after serves it.
    // Past the age an edit stops a build at, with the design reader built
    // again after it (any edit but a design's builds it), and the build
    // still running (held at its end). The reader's build is a cargo build
    // on the shared target/, queued behind its lock while another build
    // holds it: back after the full build has ended, the step is staged again.
    let batches, settled, asideBefore;
    for (let tries = 0; ; tries++) {
      if (tries === 5) throw new Error(`the design reader was not built again while a full build ran, in 5 tries: ${said().slice(-2000)}`);
      batches = count(/edit → batch ready .*no build/g); settled = count(/edit → plan ready|edit → checked/g);
      asideBefore = count(/edit → a build superseded/g);
      const readers = count(/design reader ready/g);
      writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: `Stopped again ${tries}`, app: { id: 'com.exact.stopped', name: 'Stopped' } }));
      await new Promise(ok => setTimeout(ok, 2200));
      await until(() => count(/design reader ready/g) > readers, 300_000);
      if (count(/edit → plan ready|edit → checked/g) === settled) break;
    }
    const full = Date.now();
    writeFileSync(join(dir, 'app.forma'), design('#404040'));
    await until(() => count(/edit → batch ready .*no build/g) > batches, 60_000);
    expect(Date.now() - full).toBeLessThan(3000);
    await until(() => count(/edit → plan ready|edit → checked/g) > settled, 300_000);
    expect(count(/edit → a build superseded/g)).toBe(asideBefore + 1);
    expect(JSON.stringify(await (await fetch(`${base}/__dev/design`)).json())).toContain('#404040');
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test('the JS loop: a value-only design edit while the design reader builds waits for it, then is a batch, no full build', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-reader-wait-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Reader wait', app: { id: 'com.exact.readerwait', name: 'Reader wait' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    column background-color="surface.card" min-height=40\n      text "qz-wait"\n');
  const design = (light) => `forma readerwait\n\ntheme\n  surface.card = light-dark(${light}, #16181c)\n`;
  writeFileSync(join(dir, 'app.forma'), design('#f1f3f5'));
  let loop = null;
  try {
    // The reader's first build is taken 3 minutes after cargo ends (a reader building again after any other edit, on
    // a loaded machine): the app's first build is ready well before it.
    loop = await devLoop(dir, { EXACT_DEV_READER_HOLD_MS: '180000' });
    const { said } = loop;
    const count = (re) => (said().match(re) ?? []).length;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    await until(() => said().includes('plan ready'), 900_000);
    if (said().includes('design reader ready')) throw new Error(`the reader was ready before the first build; the hold is too short: ${said().slice(-1500)}`);
    writeFileSync(join(dir, 'app.forma'), design('#e0f7e9'));
    await until(() => said().includes('edit → waiting for the design reader'), 60_000);
    // Nothing builds while it waits.
    expect(count(/edit → building/g)).toBe(0);
    await until(() => said().includes('design reader ready'), 400_000);
    await until(() => count(/edit → batch ready/g) >= 1, 120_000);
    // The edit's batch came after the reader, then the one build that checks it, and no other.
    expect(said().indexOf('design reader ready')).toBeLessThan(said().indexOf('edit → batch ready'));
    await until(() => count(/edit → checked/g) >= 1, 300_000);
    expect(count(/edit → building \(checking the batch\)/g)).toBe(1);
    expect(count(/edit → building/g)).toBe(1);
    expect(count(/edit → plan ready/g)).toBe(0);
    expect(JSON.stringify(await (await fetch(`${loop.base}/__dev/design`)).json())).toContain('e0f7e9');
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test("the JS loop: an edit to theme-fixture's `when` token builds with setMode and its pins, and a refused one says the design's refusal", async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-axes-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Axes', app: { id: 'com.exact.axes', name: 'Axes' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  action dim\n    setMode("dark-style", "dim")\n  view\n    column background-color="surface.page" min-height=40\n      button press=dim\n        text "dim"\n      column -exact-mode="dark-style=black" background-color="surface.page"\n        text "pinned"\n');
  const design = readFileSync(resolve(ROOT, 'apps/theme-fixture/app.forma'), 'utf8');
  const page = '  surface.page = light-dark(#ffffff, #000000)\n    when dark-style=dim: light-dark(#ffffff, #15202b)\n';
  expect(design).toContain(page);
  writeFileSync(join(dir, 'app.forma'), design);
  let loop = null;
  try {
    loop = await devLoop(dir);
    const { said, base } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    const count = (re) => (said().match(re) ?? []).length;
    await until(() => said().includes('plan ready') && said().includes('design reader ready'), 900_000);
    // The `when` line's dark side: its batch, then its build, which checks
    // setMode and the pin against the design's axes.
    const checks = count(/edit → checked|edit → batch ready .*built in|edit → plan ready/g);
    writeFileSync(join(dir, 'app.forma'), design.replace(page, page.replace('#15202b', '#16212c')));
    await until(() => count(/edit → checked|edit → batch ready .*built in|edit → plan ready/g) > checks || said().includes('build failed'), 300_000);
    expect(said()).not.toContain('build failed');
    expect(JSON.stringify(await (await fetch(`${base}/__dev/design`)).json())).toContain('#16212c');
    // The default's light side alone, which `under scheme=dark` refuses:
    // the build says so, and not that the design declares no axes.
    writeFileSync(join(dir, 'app.forma'), design.replace(page, page.replace('#ffffff, #000000', '#fefefe, #000000')));
    await until(() => said().includes('build failed') && said().includes('[forma-'), 300_000);
    expect(said()).toContain('forma-under-light');
    expect(said()).not.toContain('declares no axes');
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test('the JS loop: two loops on two apps under one target/ build at once, each into its own stage', async () => {
  const app = (name) => {
    const dir = mkdtempSync(join(tmpdir(), `exact-stage-${name}-`));
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name, app: { id: `com.exact.${name}`, name } }));
    writeFileSync(join(dir, 'app.contract'), `component App\n  view\n    text "${name}"\n`);
    writeFileSync(join(dir, 'app.forma'), `forma ${name}\n\ntheme\n  ink = #000000\n`);
    return dir;
  };
  const dirs = [app('stagea'), app('stageb')];
  let loops = [];
  try {
    // The first's end held 8 s: the second's whole build lands inside it (one stage between them was emptied under
    // the first, which then built nothing, or the other's files).
    // One target/ for both, named (an app outside a workspace otherwise gets <its dir>/target).
    const target = process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target');
    loops = await Promise.all([devLoop(dirs[0], { EXACT_DEV_BUILD_HOLD_MS: '8000', CARGO_TARGET_DIR: target }), devLoop(dirs[1], { CARGO_TARGET_DIR: target })]);
    for (const [i, name] of ['stagea', 'stageb'].entries()) {
      expect(loops[i].said()).not.toContain('does not build');
      const receipt = JSON.parse(readFileSync(join(loops[i].dist, '.exact-build.json'), 'utf8'));
      // Its sources as `serve` lists them, so `serve` on the loop's directory builds nothing again (expo/exact#415).
      expect([receipt.app.id, receipt.sources]).toEqual([`com.exact.${name}`, appSources(dirs[i], resolve(loops[i].dist)).digest]);
    }
  } finally {
    await Promise.all(loops.map(l => l.stop()));
    for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);


test('the JS loop rebuilds when a navigation source or grant admission changes', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-navigation-watch-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Navigation watch', app: { id: 'test.navigation-watch', name: 'Navigation watch' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Ready"\n');
  const sources = ['host/web/navigation/routes.js', 'host/web/grant-admission.js'].map(file => resolve(ROOT, file));
  const stamps = sources.map(path => statSync(path));
  let loop;
  try {
    loop = await devLoop(dir);
    for (const source of sources) {
      const before = loop.said().length;
      const now = new Date();
      utimesSync(source, now, now);
      const deadline = performance.now() + 30_000;
      while (!loop.said().slice(before).includes('edit → plan ready')) {
        if (performance.now() > deadline) throw new Error(`no rebuild after ${source}: ${loop.said().slice(before)}`);
        await Bun.sleep(50);
      }
      expect(loop.said().slice(before)).toContain('edit → plan ready');
    }
  } finally {
    await loop?.stop();
    sources.forEach((path, i) => utimesSync(path, stamps[i].atime, stamps[i].mtime));
    rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

// A Contract library the app depends on as `file:./packages/ui` is a copy in node_modules; the loop's rebuild runs
// host/web-js/build.mjs directly, which installs it again before it reads it (#115).
test("the JS loop: an edit to a copied file: library rebuilds with the library's new text", async () => {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'exact-file-library-')));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Library', app: { id: 'test.file-library', name: 'Library' } }));
  writeFileSync(join(dir, 'app.contract'), 'use Badge from "@me/ui"\n\ncomponent App\n  view\n    Badge()\n');
  writeFileSync(join(dir, 'package.json'), JSON.stringify({ name: 'library', private: true, dependencies: { '@me/ui': 'file:./packages/ui' } }));
  mkdirSync(join(dir, 'packages/ui'), { recursive: true });
  writeFileSync(join(dir, 'packages/ui/package.json'), '{"name":"@me/ui","version":"1.0.0"}');
  const badge = (text) => `component Badge\n  view\n    text "${text}"\n`;
  writeFileSync(join(dir, 'packages/ui/index.contract'), badge('qz-lib-first'));
  const bun = spawnSync(process.execPath, ['install'], { cwd: dir, encoding: 'utf8' });
  expect(bun.status).toBe(0);
  let loop = null;
  try {
    loop = await devLoop(dir);
    const { said, dist } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    const holds = (text) => { const walk = (d) => readdirSync(d, { withFileTypes: true }).some(e => e.isDirectory() ? walk(join(d, e.name)) : readFileSync(join(d, e.name)).includes(text)); return walk(dist); };
    await until(() => said().includes('plan ready') && said().includes('design reader ready') && holds('qz-lib-first'), 900_000);
    writeFileSync(join(dir, 'packages/ui/index.contract'), badge('qz-lib-second'));
    await until(() => holds('qz-lib-second'), 120_000);
    expect(holds('qz-lib-first')).toBe(false);
    expect(readFileSync(join(dir, 'node_modules/@me/ui/index.contract'), 'utf8')).toBe(badge('qz-lib-second'));
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
    // The lock the reinstall took, kept in TMPDIR by the app's path (filesystem.mjs `installLock`).
    rmSync(installLock(dir), { force: true });
  }
}, 1_200_000);


test('real loops keep their pages while test, verify and agent build the same app privately', async () => {
  const dir = mkdtempSync(resolve(ROOT, 'target/receipt-entrypoints-')), loops = [];
  try {
    const apps = ['one', 'two'].map(name => resolve(dir, name));
    for (const [n, app] of apps.entries()) {
      mkdirSync(app);
      writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: `Receipt ${n}`, app: { id: `test.receipt${n}`, name: `Receipt ${n}` } }));
      writeFileSync(resolve(app, 'app.contract'), `component App\n  view\n    text "Ready ${n}" testId="ready"\n`);
      writeFileSync(resolve(app, 'app.test.contract'), `test "ready"\n  expect text "ready" == "Ready ${n}"\n`);
      writeFileSync(resolve(app, 'exact.mjs'), `import {run} from ${JSON.stringify(resolve(ROOT, 'scripts/app-cli.mjs'))};\nprocess.exitCode = await run({dir:import.meta.dir, name:${JSON.stringify(app.split("/").at(-1))}, exact2:${JSON.stringify(ROOT)}, args:process.argv.slice(2)});\n`);
    }
    loops.push(...await Promise.all(apps.map(app => devLoop(app, { EXACT_WEB_DIST: '' }))));
    const receipts = loops.map(loop => readFileSync(resolve(loop.dist, '.exact-build.json'), 'utf8'));
    const checks = ['test', 'agent', 'verify'].map(verb => {
      const child = spawnOwned(process.execPath, [resolve(apps[0], 'exact.mjs'), verb, 'web', ...(verb === 'agent' ? ['tree'] : ['--json'])],
        { cwd: apps[0], env: { ...process.env, EXACT_WEB_DIST: '', EXACT_BAKE_OUTPUT: '' }, stdio: ['ignore', 'pipe', 'pipe'] });
      let output = ''; for (const stream of [child.stdout, child.stderr]) stream.on('data', bytes => { output += bytes; });
      return new Promise((ok, fail) => { child.once('error', fail); child.once('exit', code => ok({ verb, code, output })); });
    });
    writeFileSync(resolve(apps[0], 'unused.contract'), 'component Unused\n  text "not imported"\n');
    for (const result of await Promise.all(checks)) expect(result.code, result.verb + ': ' + result.output).toBe(0);
    for (const [n, loop] of loops.entries()) {
      const page = await (await fetch(loop.base)).text();
      expect(page).toContain('/__dev/page');
      expect(JSON.parse(readFileSync(resolve(loop.dist, '.exact-build.json'))).app).toEqual(JSON.parse(receipts[n]).app);
      expect(readFileSync(resolve(loop.dist, 'index.html'), 'utf8')).toContain(`Receipt ${n}`);
    }
    const runs = readFileSync(resolve(apps[0], '.exact/runs.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line));
    expect(new Set(runs.map(run => run.receipt)).size).toBeGreaterThanOrEqual(3);
    expect(runs.every(run => run.receipt && readFileSync(run.receipt, 'utf8').includes('"reads"'))).toBe(true);
  } finally {
    await Promise.all(loops.map(loop => loop.stop())); rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

test('the JS watcher rebuilds for included Rust files and declared inputs with output extensions', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-output-watch-'));
  const manifest = { gpu: { shaderRoots: ['shader-gen'] }, host: { macos: { resources: [{ from: 'declared.out' }, { from: 'tools/declared.err' }] } } };
  const app = { dir, manifest };
  const source = readFileSync(new URL('./dev.mjs', import.meta.url), 'utf8');
  const begin = source.indexOf('  const outputIsInput = (name)');
  const end = source.indexOf('  // The resident loop', begin);
  expect(begin).toBeGreaterThan(0);
  expect(end).toBeGreaterThan(begin);
  try {
    for (const name of ['local-crate/src', 'assets', 'shader-gen', 'tools']) mkdirSync(join(dir, name), { recursive: true });
    writeFileSync(join(dir, 'app.json'), JSON.stringify(manifest));
    writeFileSync(join(dir, 'local-crate/Cargo.toml'), '[package]\nname = "included"\nversion = "0.1.0"\n');
    writeFileSync(join(dir, 'local-crate/src/lib.rs'), 'pub const MESSAGE: &str = include_str!("messages.err");\n');
    let pending, rebuilds = 0;
    const context = {
      app, OUTPUT, INPUT_TREE, shaderWatchRoots, within, existsSync, dirname,
      stopping: false, skipped: /^\.exact(?:\/|$)/, contractInput: () => false, settingsFile: () => false,
      temporaryOfDesign: () => false, movedSince: () => true, since: Date.now() + 1000,
      resolve, statSync, timer: null, saved: 0, edited: new Set(), residentChild: null,
      starting: { failure: null }, inputs: 0, movedSinceBuild: new Set(), appeared: new Set(),
      stopYoungBuild() {}, clearTimeout() {}, setTimeout: callback => { pending = callback; return 1; },
      rebuild: () => { rebuilds++; },
    };
    runInNewContext(source.slice(begin, end) + '\nglobalThis.notify = changed(app.dir);', context);
    for (const name of ['app.contract', 'app.forma', 'app.ts', 'unused.ts', 'helper.js', 'local-crate/src/messages.err', 'local-crate/src/captured.out', 'assets/captured.err', 'shader-gen/captured.out', 'declared.out', 'tools/declared.err']) {
      writeFileSync(join(dir, name), 'edited');
      context.notify('change', name);
      expect(context.edited.has(join(dir, name)), name).toBe(true);
      expect(pending, name).toBeFunction();
      pending(); pending = null;
    }
    expect(rebuilds).toBe(11);
    for (const name of ['macos.out', 'macos.err', 'other.OUT', '.exact/evidence.err']) {
      context.notify('change', name);
      expect(context.edited.has(join(dir, name)), name).toBe(false);
      expect(pending, name).toBeNull();
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
