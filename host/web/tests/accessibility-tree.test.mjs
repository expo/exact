// @ref LLP 1080.002 §3 stage 1 — `tree --ax` on the web, the oracle: the
// fixture's controls as Chrome exposes them, joined to their views; the
// findings caught through the real collector and gone once repaired; a
// protected value omitted; the wasm and JS targets agreeing by testId; and
// Caltrain with complete coverage and no findings. It compiles the fixture
// and needs a wasm Caltrain dist, so the async lane's web-build-test step
// runs it; the glue step skips it.
import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { open, assertWebDistApp } from '../../../scripts/agent.mjs';
import { chromium, refuseStale, webChanges } from '../../../scripts/agent-launch.mjs';
import { axClean, axParity } from '../../../scripts/agent-ax.mjs';
import { resolveApp } from '../../../scripts/app.mjs';
import { jsTargetBuild } from '../serve.mjs';
import { need, notApplicable } from '../../../scripts/coverage.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('../../..', import.meta.url)));
const compiler = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'), 'debug', process.platform === 'win32' ? 'contract.exe' : 'contract');
const app = resolveApp('caltrain');
const wasmDist = resolve(process.env.EXACT_WEB_DIST ?? resolve(ROOT, 'host/web/dist'));
const jsDist = process.env.EXACT_WEB_JS_DIST ? resolve(process.env.EXACT_WEB_JS_DIST) : null;

async function prerequisite() {
  const { unavailable } = chromium();
  if (unavailable) return ['chrome', unavailable];
  try {
    await assertWebDistApp(wasmDist, app);
    if (jsTargetBuild(wasmDist)) return ['fixture', `${wasmDist} is a JS-target build; this test reads the wasm target (bun host/web/build.mjs caltrain-web --wasm)`];
    refuseStale('web', resolve(wasmDist, '.exact-build.json'), webChanges(wasmDist, app).all, 'bun host/web/build.mjs caltrain-web --wasm');
  } catch (error) { return ['fixture', error.message]; }
  return null;
}
// The async glue step does not build apps; the web-ax-test step runs these, with EXACT_REQUIRED.
const missing = process.env.EXACT_GLUE_FAST === '1' ? null : await prerequisite();
const check = process.env.EXACT_GLUE_FAST === '1' ? notApplicable('the async glue step does not build apps; the web-ax-test step runs this test')
  : need(...(missing ?? ['chrome', null]));

let tmp, plan;
function fixture() {
  if (plan) return plan;
  tmp = mkdtempSync(resolve(tmpdir(), 'exact-ax-'));
  plan = resolve(tmp, 'accessibility.plan');
  const c = spawnSync(compiler, ['build', resolve(ROOT, 'contract/corpus/accessibility.contract'), '-o', plan], { cwd: ROOT, encoding: 'utf8' });
  if (c.status !== 0) throw new Error('the accessibility fixture does not compile: ' + c.stderr);
  return plan;
}
const CONTROLS = { first: ['button', 'Increment'], other: ['button', 'Other'], toggle: ['button', 'Toggle'], 'name-it': ['button', 'Name it'],
  chart: ['image', 'Chart'], heading: ['heading', 'Section'], guide: ['link', 'guide'], check: ['checkbox', 'Checked'], secret: ['textbox', 'Secret'] };
const find = (r, testId) => r.ax.elements.find(e => e.testId === testId && e.via !== 'ancestor');
const kinds = r => r.ax.findings.map(f => `${f.kind} ${f.testId}`).sort();

async function fixtureControls(s) {
  const r = await s.tree(null, { ax: true });
  expect(r.ax.source).toBe('chrome-cdp');
  expect(r.ax.coverage.complete).toBe(true);
  for (const [testId, [role, name]] of Object.entries(CONTROLS)) {
    const e = find(r, testId);
    expect(e && [e.role, e.name, e.via === 'self' || e.via === 'owner']).toEqual([role, name, true]);
  }
  // LLP 1115.000 D12.5: a heading is `role="heading"`; an `aria-level` alone is words, and no element in Chrome's tree
  // is a heading named by them.
  expect(find(r, 'heading').states.level).toBe(2);
  expect(r.ax.elements.filter(e => e.testId === 'level-only' || e.name === 'Level only').map(e => e.role).filter(role => role === 'heading')).toEqual([]);
  expect(r.ax.elements.some(e => e.name === 'Level only')).toBe(true);
  // Coverage: one element per unignored node Chrome reported, the dropped kinds aside.
  const { nodes } = await s.carrier.call('Accessibility.getFullAXTree');
  expect(r.ax.elements.length).toBe(nodes.filter(n => !n.ignored && !['InlineTextBox', 'RootWebArea'].includes(n.role?.value)).length);
  const byId = new Map(nodes.map(n => [n.nodeId, n]));
  const destinations = nodes.find(n => !n.ignored && n.role?.value === 'list' && n.name?.value === 'Destinations');
  const rows = destinations.childIds.map(id => byId.get(id)).filter(n => !n.ignored);
  expect(rows.map(n => n.role.value)).toEqual(['listitem', 'listitem', 'listitem']);
  expect(rows.map(n => n.childIds.map(id => byId.get(id)).find(c => !c.ignored && c.role?.value === 'link')?.name.value)).toEqual(['Orchard', 'Harbor', 'Meadow']);
  // The decoration is aria-hidden, the inert box is inert: neither is exposed.
  for (const t of ['decoration', 'inert-box', 'inert-button']) expect(find(r, t)).toBeUndefined();
  // A focusable button inside an image role: present exactly when Chrome exposes it.
  const raw = nodes.some(n => !n.ignored && n.role?.value === 'button' && n.name?.value === 'Inside');
  expect(Boolean(find(r, 'chart-button'))).toBe(raw);
  return r;
}

check('tree --ax reads Chrome\'s tree for the fixture, and its findings fire and clear through the collector', async () => {
  const s = await open({ host: 'web', plan: fixture(), webDist: wasmDist });
  try {
    let r = await fixtureControls(s);
    expect(kinds(r)).toEqual(['unnamed unnamed']);
    expect(axClean(r, Object.keys(CONTROLS))).toBe(true);
    await s.tap('name-it');
    r = await s.tree(null, { ax: true });
    expect(find(r, 'unnamed')?.name).toBe('Add');
    expect(r.ax.findings).toEqual([]);
    // A negative through the real collector: the page loses `inert` while the plan still says inert.
    const box = (await s.tree('inert-box', { shallow: true })).nodes[0].id;
    await s.carrier.evaluate(`exact.views.get(${box}).removeAttribute('inert')`);
    r = await s.tree(null, { ax: true });
    expect(kinds(r)).toEqual(['exposed-hidden inert-button']);
    await s.carrier.evaluate(`exact.views.get(${box}).setAttribute('inert', '')`);
    expect((await s.tree(null, { ax: true })).ax.findings).toEqual([]);
    // A protected value is omitted, never read from the editor.
    await s.type('secret', 'hunter2');
    const secret = find(await s.tree(null, { ax: true }), 'secret');
    expect([secret.value, secret.states.protected]).toEqual([undefined, true]);
    // A targeted read keeps the target's subtree and the chain above it.
    const t = await s.tree('chart', { ax: true });
    expect(t.ax.elements.every(e => e.testId === 'chart' || e.id !== null)).toBe(true);
    expect(t.ax.ancestors).toBeArray();
  } finally { await s.close(); }
}, 180000);

check('tree --ax: a reload between the brackets is retried, never joined across documents', async () => {
  const s = await open({ host: 'web', plan: fixture(), webDist: wasmDist });
  try {
    const ask = s.carrier.ask;
    let reloads = 0;
    s.carrier.ask = async req => { const r = await ask(req); if (req.op === 'axStamp' && reloads < 6) { reloads++; return { ...r, nonce: r.nonce + reloads }; } return r; };
    const r = await s.tree(null, { ax: true });
    expect(r.spanned).toBe(true);
    s.carrier.ask = ask;
    expect((await s.tree(null, { ax: true })).spanned).toBeUndefined();
  } finally { await s.close(); }
}, 120000);

(jsDist || missing || process.env.EXACT_GLUE_FAST === '1' ? check : need('fixture', 'no JS Caltrain dist: name one with EXACT_WEB_JS_DIST'))('tree --ax: the wasm and JS targets agree by testId on the fixture controls', async () => {
  const read = async dist => { const s = await open({ host: 'web', plan: fixture(), webDist: dist }); try { return await s.tree(null, { ax: true }); } finally { await s.close(); } };
  const [wasm, js] = [await read(wasmDist), await read(jsDist)];
  const { findings, unjoined } = axParity(wasm, js, Object.keys(CONTROLS));
  expect({ findings, unjoined }).toEqual({ findings: [], unjoined: [] });
}, 240000);

check('tree --ax on Caltrain: complete coverage, the station controls named, no findings', async () => {
  const s = await open({ host: 'web', webDist: wasmDist });
  try {
    await s.clock('settle');
    let r = await s.tree(null, { ax: true });
    expect(axClean(r, ['change-station'])).toBe(true);
    expect(r.ax.findings).toEqual([]);
    await s.tap('change-station'); await s.clock('settle');
    r = await s.tree(null, { ax: true });
    expect(axClean(r, ['station-search'])).toBe(true);
    expect(find(r, 'station-search')?.name).toBe('Search stations');
    expect(r.ax.findings).toEqual([]);
  } finally { await s.close(); if (tmp) rmSync(tmp, { recursive: true, force: true }); }
}, 120000);

const target = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'));
const splitCheck = missing || process.env.EXACT_GLUE_FAST === '1' ? check : need('fixture',
  JSON.parse(readFileSync(resolve(wasmDist, 'bake.json'), 'utf8')).binary?.configuration?.flags?.EXACT_WEB_LINK === 'all'
    ? null : 'build Caltrain with EXACT_WEB_LINK=all bun host/web/build.mjs caltrain-web --wasm');

splitCheck('chromium: wasm and JS agree on split activity and tree targets', async () => {
  const dir = mkdtempSync(resolve(target, 'split-activity-'));
  let server, session;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({
      name: 'Split Activity', app: { id: 'com.exact.caltrain', name: 'Split Activity' }, host: { web: {} },
    }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  tab items "/"
    person "/people/:id"
    edit "/edit"
  tab saved "/saved"

component App
  state recorded = ""
  action show(id: string)
    nav = push(nav, path("person", id))
  action pick(name: string)
    nav = select(nav, name)
  action modal
    nav = push(nav, "/edit")
  action back
    nav = back(nav)
  action record(name: string)
    recorded = name
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" width="100%" height="100%" display="flex" flex-direction="column"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=\`panel-\${t.name}\` flex=1 position="relative"
          for e in t.stack key=e.id
            column navigationKey=\`\${e.id}\` navigationPresentation=(e.name == "edit" ? "modal" : "") testId=\`route-\${e.name}\` position="absolute" inset=0
              if e.name == "items"
                list
                  button press=show("alpha") testId="row-alpha"
                    text "Alpha"
                  button press=show("beta") testId="row-beta"
                    text "Beta"
              if e.name == "person"
                button press=modal testId="open-modal"
                  text "Edit"
              if e.name == "edit"
                button press=back id="back" testId="close-modal"
                  text "Close"
              box position="relative" width=240 height=100
                text testId="flow-paragraph" width=240 height=100 font-size=16 line-height="24px"
                  text testId="flow-run"
                    text e.name testId="flow-shared"
                box position="absolute" left=80 top=0 width=20 height=20 wrap-flow="both" shape-outside="circle()"
              button press=record(e.name) testId="shared"
                text e.name
      row role="tablist"
        button role="tab" aria-controls="panel-items" aria-selected=(nav.tab == "items") press=pick("items") testId="tab-items"
          text "Items"
        button role="tab" aria-controls="panel-saved" aria-selected=(nav.tab == "saved") press=pick("saved") testId="tab-saved"
          text "Saved"
`);
    const plan = resolve(dir, 'app.plan');
    const compiled = spawnSync(compiler, ['build', resolve(dir, 'app.contract'), '-o', plan], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    const jsDist = resolve(dir, 'dist');
    const built = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'split-activity', '--out', jsDist, '--render', 'none'], {
      cwd: ROOT, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([built.exited, new Response(built.stdout).text(), new Response(built.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    const results = [];
    for (const [kind, dist] of [['JS', jsDist], ['wasm', wasmDist]]) {
      server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
        const path = new URL(req.url).pathname;
        const file = Bun.file(path === '/__plan' ? plan : resolve(dist, path === '/' ? 'index.html' : path.slice(1)));
        return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
      } });
      session = await open({ host: 'web', url: server.url.href, size: [1000, 800] });
      if (kind === 'wasm') await session.carrier.evaluate("fetch('/__plan').then(r => r.arrayBuffer()).then(b => exact.reload(new Uint8Array(b), true))");
      const snapshots = [];
      const read = async (listFlags, selected) => {
        await session.clock('settle');
        const nodes = (await session.tree()).nodes;
        const flags = id => nodes.filter(n => n.props.testId === id).map(n => [!!n.inactive, !!n.hidden]);
        expect(flags('row-beta'), kind).toEqual([listFlags]);
        const copies = nodes.filter(n => n.props.testId === 'shared');
        const active = copies.find(n => !n.inactive) ?? copies[0];
        expect((await session.tree('shared', { shallow: true })).nodes[0].id, kind).toBe(active.id);
        expect((await session.tree('shared')).roots, kind).toEqual([active.id]);
        const runs = nodes.filter(n => n.props.testId === 'flow-shared');
        const activeRun = runs.find(n => !n.inactive) ?? runs[0];
        expect(activeRun.props.text, kind).toBe(selected);
        const byId = new Map(nodes.map(n => [n.id, n]));
        const subtree = node => [node, ...(node.children ?? []).flatMap(id => subtree(byId.get(id)))];
        const summary = node => [node.id, node.children, !!node.inactive, !!node.hidden];
        for (const name of ['flow-shared', 'flow-run', 'flow-paragraph']) {
          const candidates = nodes.filter(n => n.props.testId === name);
          const expected = candidates.find(n => !n.inactive) ?? candidates[0];
          for (const [target, hit] of [[name, expected], ...candidates.map(n => [n.id, n])]) {
            for (const shallow of [false, true]) {
              const reply = await session.tree(target, { shallow });
              const context = `${kind} ${name} ${selected} target=${target} shallow=${shallow}`;
              expect(reply.roots, context).toEqual([hit.id]);
              expect(reply.nodes.map(summary), context).toEqual((shallow ? [hit] : subtree(hit)).map(summary));
            }
          }
        }
        if (selected === 'items' || selected === 'person') {
          const run = nodes.find(n => n.props.testId === 'flow-run' && !n.inactive);
          expect(await session.carrier.evaluate(`!exact.views.get(${run.id}).isConnected`), kind).toBe(true);
        }
        await session.tap('shared');
        expect((await session.state()).slots.recorded, kind).toBe(selected);
        snapshots.push(['row-beta', ...flags('row-beta'), ...copies.map(n => [!!n.inactive, !!n.hidden]),
          ...runs.map(n => [!!n.inactive, !!n.hidden])]);
      };
      await session.tap('row-alpha');
      for (const width of [1000, 420, 1000]) {
        await session.resize(width, 800);
        await read([width < 900, width < 900], width < 900 ? 'person' : 'items');
      }
      await session.tap('row-beta');
      expect((await session.state()).navigation.url, kind).toBe('/people/beta');
      await read([false, false], 'items');
      await session.tap('tab-saved');
      await read([true, true], 'saved');
      await session.tap('tab-items');
      await read([false, false], 'items');
      await session.tap('open-modal');
      await read([true, false], 'edit');
      await session.resize(420, 800);
      await read([true, true], 'edit');
      await session.resize(1000, 800);
      await session.tap('close-modal');
      await read([false, false], 'items');
      results.push(snapshots);
      await session.close(); session = null;
      server.stop(true); server = null;
    }
    expect(results[1]).toEqual(results[0]);
  } finally {
    await session?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);
