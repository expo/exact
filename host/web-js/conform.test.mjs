import { test, expect } from 'bun:test';
import { conformArgs } from './conform-args.mjs';
import { clockOnAll, clockVerdict, parseStep } from './conform-steps.mjs';
import { open } from '../../scripts/agent.mjs';
import { launchFacts, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { browserMissing } from '../../scripts/agent-playwright.mjs';
import { need } from '../../scripts/coverage.mjs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';

// Load the comparisons without starting the harness's builds and browser drives.
const comparisonSource = readFileSync(resolve(import.meta.dir, 'conform.mjs'), 'utf8');
test('TypeScript fixture JS reuse checks the parent source fingerprint', async () => {
  const sourceDeclaration = comparisonSource.slice(comparisonSource.indexOf('const sourceTree ='), comparisonSource.indexOf('const fileDigest ='));
  const fingerprint = (env, git) => new Function('process', 'spawnSync', 'root', 'createHash', sourceDeclaration + '; return sourceTree;')(
    { env }, git, '/checkout', createHash,
  );
  const parent = diff => fingerprint({}, (_command, args) => ({ stdout: args[0] === 'rev-parse' ? 'head' : diff }));
  const noGit = () => ({ status: 128, stdout: '', stderr: 'fatal: invalid gitfile format' });
  const fixtureSource = comparisonSource.slice(comparisonSource.indexOf('async function typescriptFixture('), comparisonSource.indexOf('// ---------------------------------------------------------------- the run'));
  const reuseSource = comparisonSource.slice(comparisonSource.indexOf('  const earlier = jsFrom'), comparisonSource.indexOf('  // A route that paints its boot document'));
  const files = new Map(), report = { targets: {} }, writes = [], builds = [];
  const jsFrom = '/previous/comparison', out = '/current/comparison', t = { name: 'fieldnotes', app: 'fieldnotes', wasm: '/wasm' };
  const read = path => files.get(path), write = (path, value) => { files.set(path, value); writes.push(path); };
  const reuse = async source => {
    const deps = {
      t: { ...t }, plan: '/wasm/app.plan', inputs: { plan: 'plan', data: 'data', source }, jsFrom, out, root: '/checkout', JS_BUILT: '.conform-js.json',
      resolve, existsSync: path => files.has(path), readFileSync: read, writeFileSync: write, report,
      process: { stderr: { write() {} } },
      spawned: async () => { builds.push(source); return { status: 0, stderr: '' }; }, fail: () => { throw new Error('JS build failed'); },
    };
    await new Function(...Object.keys(deps), `return (async () => { ${reuseSource} })();`)(...Object.values(deps));
  };
  const carry = async sourceTree => {
    const deps = {
      out: '/fixture-run', root: '/checkout', sdir: '/checkout/host/web-js/conformance', sourceTree,
      resolve, dirname, basename, mkdirSync() {}, rmSync() {}, cpSync() {}, symlinkSync() {}, appendFileSync() {},
      existsSync: path => files.has(path) || path.endsWith('/report.json'), writeFileSync: write,
      readFileSync: path => path.endsWith('/app.ts') ? 'export const appId = "fieldnotes";\nexport const grants = {};'
        : path.endsWith('/app.json') ? '{}' : path.endsWith('/report.json') ? '{"targets":{},"steps":[],"failures":[],"known":[]}' : '',
      spawnSync: () => ({ status: 0, stdout: '', stderr: '' }), process: { env: {}, execPath: process.execPath },
      crossBrowser: 'firefox', jsFrom: '/previous', linuxRef: false, argv: [],
      spawned: async (_command, _args, { env }) => {
        const source = fingerprint(env, noGit);
        expect(source).toBe(sourceTree);
        await reuse(source);
        return { status: 0, stderr: '', stdout: '' };
      },
    };
    const fixture = new Function(...Object.keys(deps), fixtureSource + '; return typescriptFixture;')(...Object.values(deps));
    await fixture({ name: 'synthetic-storage-background', app: 'fieldnotes', contract: '/fixture/app.contract' },
      { targets: {}, steps: [], failures: [], known: [] }, (step, what) => { throw new Error(`${step}: ${what}`); });
  };
  const before = parent(''), after = parent('host/web-js/ts-data.js changed');
  expect(before).not.toBe(after);
  files.set(resolve(jsFrom, 'dist', t.name, '.conform-js.json'), JSON.stringify({ plan: 'plan', data: 'data', source: before, warnings: 0 }));
  await carry(before);
  expect(builds).toEqual([]);
  expect(report.targets.fieldnotes.sharedFrom).toBe(jsFrom);
  await carry(after);
  expect(builds).toEqual([after]);
  expect(report.targets.fieldnotes.sharedFrom).toBeUndefined();
  const artifact = resolve(out, 'dist', t.name, '.conform-js.json');
  expect(writes).toContain(artifact);
  expect(JSON.parse(read(artifact)).source).toBe(after);
});

test('TypeScript fixture reuse removes deleted source files and keeps build artifacts', async () => {
  const target = resolve(import.meta.dir, '../../target'); mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(resolve(target, 'conform-source-'));
  try {
    const root = resolve(dir, 'checkout'), out = resolve(dir, 'out'), sdir = resolve(root, 'host/web-js/conformance');
    const t = { name: 'synthetic-storage-background', app: 'fieldnotes', contract: resolve(sdir, 'storage-background/app.contract') };
    const inputs = {
      'apps/fieldnotes/app.ts': 'export const appId = "fieldnotes";\nexport const grants = {};\n',
      'apps/fieldnotes/app.json': '{"typescript":{"placement":"worker"}}',
      'host/web-js/ts-data.js': 'export const runtime = "current";\n',
      'host/web-js/conformance/storage-background/app.ts': 'export const answers = {};\n',
      'host/web-js/conformance/storage-background/app.contract': '// data: fieldnotes\n',
      'host/web-js/conformance/storage-background/app.test.contract': 'test "storage" {}\n',
      'host/web-js/conformance/storage-background.steps': 'clock settle\n',
    };
    for (const [file, text] of Object.entries(inputs)) {
      const path = resolve(root, file); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, text);
    }
    mkdirSync(resolve(root, 'node_modules'));
    const carrier = resolve(out, t.name, 'carrier'), copied = resolve(carrier, 'source/host/web-js/ts-data.js');
    const artifact = resolve(carrier, 'target/cached-build'), builds = [];
    const deps = {
      root, out, sdir, sourceTree: 'current-inputs', resolve, dirname, basename,
      appendFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync,
      spawnSync: () => ({ status: 0, stdout: Object.keys(inputs).join('\0'), stderr: '' }),
      process: { env: {}, execPath: process.execPath }, crossBrowser: null, jsFrom: null, linuxRef: true, argv: ['--build'],
      spawned: async () => {
        builds.push(existsSync(copied) ? readFileSync(copied, 'utf8') : null);
        const comparison = resolve(carrier, 'comparison'); mkdirSync(comparison, { recursive: true });
        writeFileSync(resolve(comparison, 'report.json'), '{"targets":{},"steps":[],"failures":[],"known":[]}');
        return { status: 0, stderr: '', stdout: '' };
      },
    };
    const fixtureSource = comparisonSource.slice(comparisonSource.indexOf('async function typescriptFixture('), comparisonSource.indexOf('// ---------------------------------------------------------------- the run'));
    const fixture = new Function(...Object.keys(deps), fixtureSource + '; return typescriptFixture;')(...Object.values(deps));
    const carry = () => fixture(t, { targets: {}, steps: [], failures: [], known: [] }, (step, what) => { throw new Error(`${step}: ${what}`); });
    await carry();
    mkdirSync(dirname(artifact), { recursive: true }); writeFileSync(artifact, 'cached');
    rmSync(resolve(root, 'host/web-js/ts-data.js'));
    await carry();
    expect(builds).toEqual([inputs['host/web-js/ts-data.js'], null]);
    expect(readFileSync(artifact, 'utf8')).toBe('cached');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('conformance compares background accounting and storage failures by operation, path and code', async () => {
  const keys = new Function(comparisonSource.match(/const STATE_KEYS = .*;/)[0] + '; return STATE_KEYS;')();
  expect(keys).toContain('background');
  const { journal, storageLine } = new Function(comparisonSource.slice(
    comparisonSource.indexOf('const BOUND ='), comparisonSource.indexOf('// A rendered page caches'),
  ) + '; return { journal, storageLine };')();
  const read = lines => journal({ logs: async () => ({ lines }) });
  const a = await read(['t=0 console: storage failed: readFile app:/data/backups/missing: ENOENT No such file or directory']);
  const b = await read(['t=0 storage failed: readFile app:/data/backups/missing: ENOENT file not found']);
  expect(a.storage).toEqual(['storage failed: readFile app:/data/backups/missing: ENOENT']);
  expect(a.storage).toEqual(b.storage);
  expect((await read(['t=0 storage failed: readFile app:/data/backups/missing: EACCES denied'])).storage).not.toEqual(a.storage);
  for (const line of ['storage failed: stat app:/data/backups/missing: ENOENT missing', 'storage failed: readFile app:/data/backups/other: ENOENT missing']) {
    expect((await read([`t=0 ${line}`])).storage).not.toEqual(a.storage);
  }
  expect(storageLine(null)).toBeNull();
  expect(storageLine('storage failed: an executor failure without a code')).toBe('storage failed: an executor failure without a code');
  // The lowercase codes (js/src/storage.js `storageCode`) keep the code and drop the message too.
  for (const code of ['failed', 'denied', 'full', 'EBUSY']) {
    const [x, y] = [await read([`t=0 storage failed: readFile app:/data/a: ${code} one host's words`]), await read([`t=0 console: storage failed: readFile app:/data/a: ${code} another's`])];
    expect(x.storage).toEqual([`storage failed: readFile app:/data/a: ${code}`]);
    expect(x.storage).toEqual(y.storage);
  }
  // A handle left open after background work failed (LLP 1097 D7) is journaled with `storage:`.
  const open = 'storage: app:/data/fieldnotes.db is still open after a failure in background work that opened it';
  expect((await read([`t=0 ${open}`])).storage).toEqual([open]);
  expect((await read([`t=0 console: ${open}`])).storage).not.toEqual((await read([])).storage);
});
const { comparisonTree, comparisonLayout, norm, diffLists, diffLayout } = new Function(comparisonSource.slice(
  comparisonSource.indexOf('async function comparisonTree('), comparisonSource.indexOf('function diffPng('),
) + '; return { comparisonTree, comparisonLayout, norm, diffLists, diffLayout };')();

async function pageSnapshot(page) {
  const [layout, document] = await comparisonLayout({
    layout: () => page.evaluate(() => {
      globalThis.exact = { views: new Map(['root', 'child'].map((testId, id) => [id, document.getElementById(testId)])) };
      return { nodes: [...exact.views].map(([id, el]) => {
        const b = el.getBoundingClientRect(); return { id, testId: el.id, x: b.x, y: b.y, w: b.width, h: b.height };
      }) };
    }),
    carrier: { evaluate: expression => page.evaluate(expression) },
  }, true);
  return { layout, document };
}

for (const engine of ['chromium', 'firefox', 'webkit']) {
  need(engine, await browserMissing(engine))(`document scroll is compared once with a testId on the root (${engine})`, async () => {
    const browser = await suiteBrowser(engine), page = await browser.newPage({ viewport: { width: 420, height: 600 } });
    try {
      await page.setContent(`<main id="root" style="width:1800px;height:1600px"><div id="child" style="margin:20px;width:80px;height:80px"></div></main>`);
      const snapshot = () => pageSnapshot(page);
      const tree = { nodes: [{ id: 0, children: [1] }, { id: 1, children: [] }] }, before = await snapshot();
      await page.evaluate(() => scrollTo(23, 80));
      await page.waitForFunction(() => scrollX === 23 && scrollY === 80);
      const after = await snapshot();
      const compare = b => diffLayout(before.layout, b.layout, tree, tree, true, 'chrome', engine, before.document, b.document);
      expect(compare(after).map(d => d.field)).toEqual(['layout.$document.sx', 'layout.$document.sy']);
      await page.locator('#root').evaluate(el => { el.style.marginLeft = '20px'; });
      expect(compare(await snapshot()).map(d => d.field)).toEqual(['layout.root.x', 'layout.$document.sx', 'layout.$document.sy']);
    } finally { await browser.close(); }
  });

  need(engine, await browserMissing(engine))(`a viewport-fixed root stays relative to the viewport (${engine})`, async () => {
    const browser = await suiteBrowser(engine), page = await browser.newPage({ viewport: { width: 420, height: 600 } });
    try {
      await page.setContent(`<body style="width:1800px;height:1600px"><main id="root" style="position:fixed;left:0;top:0;width:200px;height:200px"><div id="child" style="margin:20px;width:80px;height:80px"></div></main>`);
      const snapshot = () => pageSnapshot(page);
      const tree = { nodes: [{ id: 0, children: [1] }, { id: 1, children: [] }] }, before = await snapshot();
      await page.evaluate(() => scrollTo(23, 80));
      await page.waitForFunction(() => scrollX === 23 && scrollY === 80);
      const compare = b => diffLayout(before.layout, b.layout, tree, tree, true, 'chrome', engine, before.document, b.document);
      expect(compare(await snapshot()).map(d => d.field)).toEqual(['layout.$document.sx', 'layout.$document.sy']);
      await page.locator('#root').evaluate(el => { el.style.left = '20px'; });
      expect(compare(await snapshot()).map(d => d.field)).toEqual(['layout.root.x', 'layout.$document.sx', 'layout.$document.sy']);
    } finally { await browser.close(); }
  });
}

test('an engine launch failure stops synthetic compilation and reports only the launch', () => {
  const out = mkdtempSync(resolve(tmpdir(), 'exact-conform-preflight-'));
  try {
    for (const engine of ['firefox', 'webkit']) {
      const compiler = resolve(out, `${engine}-compiler`), destination = resolve(out, engine);
      const run = spawnSync(process.execPath, ['-e', `
        import { mock } from 'bun:test';
        import { writeFileSync } from 'node:fs';
        const processes = await import('node:child_process'), spawnSync = processes.spawnSync;
        mock.module('node:child_process', () => ({ ...processes, spawnSync(command, ...args) {
          if (command !== 'cargo') return spawnSync(command, ...args);
          writeFileSync(${JSON.stringify(compiler)}, 'compiled');
          return { status: 1, stdout: '', stderr: 'compiler must not run after failed preflight' };
        } }));
        mock.module('playwright-core', () => ({ ${engine}: { launchServer: async () => { throw new Error('fixture launch failure'); } } }));
        process.argv = [process.execPath, 'conform.mjs', '--synthetic', '--only', 'document', '--browser', ${JSON.stringify(engine)},
          '--build-js', '--build', '--strict', '--out', ${JSON.stringify(destination)}, '--wasm-root', ${JSON.stringify(resolve(out, 'wasm'))}];
        await import(${JSON.stringify(resolve(import.meta.dir, 'conform.mjs'))});
      `], { encoding: 'utf8', timeout: 15000 });
      expect(run.status, run.stderr).toBe(1);
      const report = JSON.parse(readFileSync(resolve(destination, 'report.json'), 'utf8'));
      expect(report.failures).toEqual([{ target: 'engine', step: 'launch', what: `web carrier unavailable: ${engine}: fixture launch failure` }]);
      expect(existsSync(compiler)).toBe(false);
      expect(report.targets).toEqual({});
      expect(report.steps).toEqual([]);
    }
  } finally { rmSync(out, { recursive: true, force: true }); }
}, 30000);

test('unopened and cached-hidden tab panels compare by their visible content', () => {
  const home = { id: 1, type: 'Text', depth: 1, props: { testId: 'route-home', text: 'home' } };
  const panel = { id: 2, type: 'View', depth: 0, props: { testId: 'panel-saved' }, inactive: true, hidden: true, children: [] };
  const saved = { id: 3, type: 'Text', depth: 1, props: { testId: 'route-saved', text: 'saved' }, inactive: true, hidden: true };
  const lazy = { nodes: [home, { ...panel, unopened: true }] };
  const cached = { nodes: [home, { ...panel, children: [3] }, saved] };
  const layout = tree => ({ nodes: tree.nodes.map(n => ({ id: n.id, testId: n.props.testId, x: 0, y: 0, w: 100, h: 20 })) });
  expect(norm(lazy)).toEqual(norm(cached));
  for (const relative of [false, true]) {
    expect(diffLayout(layout(lazy), layout(cached), lazy, cached, relative)).toHaveLength(0);
    expect(diffLayout(layout(cached), layout(lazy), cached, lazy, relative)).toHaveLength(0);
  }
  const selected = { nodes: cached.nodes.map(n => ({ ...n, inactive: false, hidden: false })) };
  expect(norm(selected)).toHaveLength(3);
  const moved = layout(selected);
  moved.nodes[2].x = 10;
  expect(diffLayout(layout(selected), moved, selected, selected).map(x => x.field)).toEqual(['layout.route-saved.x']);
});

test('visible unmatched content still fails conformance', () => {
  const hidden = { nodes: [{ id: 1, type: 'Text', props: { testId: 'route-saved', text: 'saved' }, hidden: true, inactive: true }] };
  const visible = { nodes: hidden.nodes.map(n => ({ ...n, hidden: false })) };
  const layout = { nodes: [{ id: 1, testId: 'route-saved', x: 0, y: 0, w: 100, h: 20 }] };
  const empty = { nodes: [] };
  expect(norm(hidden)).not.toEqual(norm(visible));
  for (const [a, b, treeA, treeB] of [
    [empty, layout, empty, visible], [layout, empty, visible, empty],
    [layout, layout, hidden, visible], [layout, layout, visible, hidden],
    [empty, layout, empty, null],
  ]) expect(diffLayout(a, b, treeA, treeB).map(x => x.field)).toEqual(['layout.route-saved.present']);
});

test('inactive content that remains visible keeps its tree and layout checks', () => {
  const tree = { nodes: [{ id: 1, type: 'View', props: { testId: 'covered-page' }, inactive: true }] };
  const layout = { nodes: [{ id: 1, testId: 'covered-page', x: 0, y: 0, w: 100, h: 20 }] };
  expect(norm(tree)).toHaveLength(1);
  expect(diffLayout(layout, { nodes: [] }, tree, { nodes: [] }).map(x => x.field)).toEqual(['layout.covered-page.present']);
  expect(diffLayout(layout, { nodes: [{ ...layout.nodes[0], h: 30 }] }, tree, tree).map(x => x.field)).toEqual(['layout.covered-page.h']);
});

test('a cached hidden copy cannot shadow the visible box with the same testId', () => {
  const visible = { id: 2, type: 'Text', props: { testId: 'title', text: 'Title' } };
  const cached = { nodes: [{ ...visible, id: 1, hidden: true }, visible] };
  const selected = { nodes: [visible] };
  const layout = { nodes: [
    { id: 1, testId: 'title', x: 0, y: 0, w: 0, h: 0 },
    { id: 2, testId: 'title', x: 10, y: 20, w: 100, h: 20 },
  ] };
  expect(norm(cached)).toEqual(norm(selected));
  expect(diffLayout(layout, { nodes: [layout.nodes[1]] }, cached, selected)).toHaveLength(0);
});

test('a visible descendant of a hidden cached panel still fails conformance', async () => {
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage();
    await page.setContent(`<div style="visibility:hidden">
      <div id="cached">cached</div>
      <div id="leak" style="visibility:visible;width:1px;height:1px;background:red"></div>
      <div style="display:none"><div id="unrendered" style="visibility:visible">unrendered</div></div>
    </div><div id="selected">selected</div>`);
    const nodes = ['cached', 'leak', 'unrendered', 'selected'].map((testId, id) => ({
      id, type: 'View', props: { testId }, hidden: testId !== 'selected', inactive: testId !== 'selected',
    }));
    await page.evaluate(nodes => { globalThis.exact = { views: new Map(nodes.map(n => [n.id, document.getElementById(n.props.testId)])) }; }, nodes);
    const session = { tree: async () => ({ nodes }), carrier: { evaluate: expression => page.evaluate(expression) } };
    const tree = await comparisonTree(session);
    const layout = await page.evaluate(() => ({ nodes: [...exact.views].map(([id, el]) => {
      const b = el.getBoundingClientRect();
      return { id, testId: el.id, x: b.x, y: b.y, w: b.width, h: b.height };
    }) }));
    expect(await page.locator('#leak').evaluate(el => el.checkVisibility({ visibilityProperty: true }))).toBe(true);
    const selected = { nodes: [nodes[3]] }, selectedLayout = { nodes: [layout.nodes[3]] };
    expect(norm(tree).map(row => row.split('|')[2])).toEqual(['leak', 'selected']);
    expect(tree.nodes.map(n => n.hidden)).toEqual([true, false, true, false]);
    expect(nodes[1].hidden).toBe(true);
    for (const relative of [false, true]) {
      expect(diffLayout(selectedLayout, layout, selected, tree, relative).map(x => x.field)).toEqual(['layout.leak.present']);
      expect(diffLayout(layout, selectedLayout, tree, selected, relative).map(x => x.field)).toEqual(['layout.leak.present']);
    }
  } finally { await browser.close(); }
});

test('differing visible folded text in a hidden cached panel fails conformance', async () => {
  const browser = await suiteBrowser('chromium');
  try {
    // A folded text is its box's `display: contents` child, with no box of its own (host/web/src/element.rs, `folds`).
    const panel = async word => {
      const page = await browser.newPage();
      await page.setContent(`<div id="panel" style="visibility:hidden">
        <div id="box" style="visibility:visible"><span id="folded" style="display:contents">${word}</span></div>
        <div><span id="hiddenFolded" style="display:contents">${word}</span></div>
        <div style="display:none"><span id="unrendered" style="display:contents;visibility:visible">${word}</span></div>
        <div style="content-visibility:hidden"><span id="skipped" style="display:contents;visibility:visible">${word}</span></div>
      </div>`);
      const ids = ['panel', 'box', 'folded', 'hiddenFolded', 'unrendered', 'skipped'];
      const nodes = ids.map((id, i) => ({
        id: i, type: id === 'panel' || id === 'box' ? 'View' : 'Text', depth: id === 'panel' ? 0 : 1,
        props: id === 'panel' || id === 'box' ? { testId: id } : { text: word }, hidden: true, inactive: true,
      }));
      await page.evaluate(ids => { globalThis.exact = { views: new Map(ids.map((id, i) => [i, document.getElementById(id)])) }; }, ids);
      return comparisonTree({ tree: async () => ({ nodes }), carrier: { evaluate: expression => page.evaluate(expression) } });
    };
    const [a, b] = [await panel('a'), await panel('b')];
    expect(a.nodes.map(n => n.hidden)).toEqual([true, false, false, true, true, true]);
    expect(norm(a)).toEqual(['1|View|box||||||', '1|Text||a|||||']);
    expect(diffLists(norm(a), norm(b), 'tree')).toEqual(['tree #1: wasm «1|Text||a|||||» js «1|Text||b|||||»']);
  } finally { await browser.close(); }
});

test('visible painted runs in a hidden cached panel still fail conformance', async () => {
  const modules = new Set(['textflow-glue.js', 'timer-glue.js', 'timer-waits-glue.js', 'timer-observations.js']);
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const name = new URL(request.url).pathname.slice(1);
    return modules.has(name) ? new Response(Bun.file(resolve(import.meta.dir, '../web', name)))
      : new Response(`<div id="context" style="visibility:hidden;position:relative;width:300px">
          <p id="paragraph" style="font:8px/12px monospace;height:24px"><span id="cached">a</span><span id="wrapper"><span
            id="leak" class="leak" style="visibility:visible">x</span></span></p>
          <div id="ball" style="position:absolute;left:0;top:0;width:20px;height:20px"></div>
        </div>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser('chromium');
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const nodes = await page.evaluate(async () => {
      const { createTextFlow } = await import('/textflow-glue.js');
      const elements = ['context', 'paragraph', 'cached', 'wrapper', 'leak', 'ball'].map(id => document.getElementById(id));
      const views = new Map(elements.map((el, id) => [id, el]));
      globalThis.exact = { views };
      const request = op => op === 0 ? { ranges: [[0, 2, 0, 2]], graphemes: [[0, 1, 0, 1], [1, 2, 1, 2]] }
        : op === 2 ? { shapes: [], complete: true, height: 12, line_height: 12,
          fragments: [{ start: 0, end: 2, utf16_start: 0, utf16_end: 2, paint_start: 0, paint_end: 2, x: 20, y: 0, width: 10, available: 280, line: 0 }] } : {};
      globalThis.flow = createTextFlow({ views, request, agentMode: true, advance() {} });
      flow.afterBatch({ ops: [{ op: 'textflow', contexts: [{ id: 0, exclusions: [5], paragraphs: [{ id: 1, definite: true }] }] }] });
      await flow.settle();
      return elements.map((el, id) => ({ id, type: id === 0 || id === 5 ? 'View' : 'Text',
        props: { testId: el.id, ...(id === 2 ? { text: 'a' } : id === 4 ? { text: 'x' } : {}) },
        hidden: true, inactive: true, unopened: id === 0,
        children: id === 0 ? [1, 5] : id === 1 ? [2, 3] : id === 3 ? [4] : [],
      }));
    });
    expect(await page.evaluate(() => exact.views.get(4).isConnected)).toBe(false);
    expect(await page.locator('[data-flow-fragment] .leak').evaluate(el => el.checkVisibility({ visibilityProperty: true }))).toBe(true);
    const session = { tree: async () => ({ nodes }), carrier: { evaluate: expression => page.evaluate(expression) } };
    const tree = await comparisonTree(session);
    expect(norm(tree).map(row => row.split('|')[2])).toEqual(['leak']);
    expect(tree.nodes.map(n => n.hidden)).toEqual([true, true, true, true, false, true]);
    const layout = await page.evaluate(() => ({ nodes: [...exact.views].map(([id, el]) => {
      const b = el.getBoundingClientRect();
      return { id, testId: el.id, x: b.x, y: b.y, w: b.width, h: b.height };
    }) }));
    for (const relative of [false, true]) {
      expect(diffLayout({ nodes: [] }, layout, { nodes: [] }, tree, relative).map(x => x.field)).toEqual(['layout.leak.present']);
      expect(diffLayout(layout, { nodes: [] }, tree, { nodes: [] }, relative).map(x => x.field)).toEqual(['layout.leak.present']);
    }
    await page.locator('[data-flow-fragment] .leak').evaluate(el => { el.style.visibility = 'hidden'; });
    expect(norm(await comparisonTree(session))).toEqual([]);
    await page.locator('#context').evaluate(el => { el.style.visibility = 'visible'; });
    const selected = { ...session, tree: async () => ({ nodes: nodes.map(n => ({ ...n, hidden: false, inactive: false, unopened: false })) }) };
    expect(norm(await comparisonTree(selected))).toHaveLength(nodes.length);
    await page.evaluate(() => flow.dispose());
  } finally { await browser?.close(); await server.stop(true); }
});

test('malformed steps fail as not a step, including steps marked refused', async () => {
  const s = await open({ reuse: { host: 'web', browser: 'chrome', launchFacts: launchFacts({}), reset() {},
    ask() { throw new Error('malformed input reached a host'); } } });
  const lines = ['refused key field Enter banana', 'refused key missing Banana', 'refused prefer prefers-color-scheme banana',
    'refused mediasession missing seekto -1', 'refused mediasession missing seekto',
    'refused prefer display-mode fullscreen segments 0x1', 'refused prefer display-mode fullscreen segments 2x1 gap -1',
    'drag a to b 300 hold -1', 'drag a 1 2 300 h0ld -1', 'refused', 'refused tpa x',
    'refused resize', 'refused resize 10', 'refused key field Enter for', 'refused key field Enter for -1',
    'refused prefer online true has-focus', 'refused clock banana', 'clock banana', 'clock +100 real',
    'refused wheel strip x', 'refused drag a 1 2 3 hold', 'refused move 1', 'refused pinch a big',
    'refused pinch a 0', 'drag a 1 2 -5', 'refused toString x', 'tap a extra', 'up extra'];
  for (const line of lines) {
    const error = await (async () => parseStep(line).run(s))().catch(e => e);
    expect(error.malformed, line).toBe(true);
    expect(error.message, line).toStartWith(`not a step: ${line}: `);
  }
});

test('malformed input fails at parse time before a browser can skip it', () => {
  for (const mark of ['', 'refused ']) for (const line of [
    'drag a 1 2 -5', 'drag a to b -1', 'drag a 1 2 300 hold -1',
    'move 1 2 -1', 'move NaN 2', 'pinch a 0', 'pinch a Infinity',
    'into missing row banana', 'into list row Start',
  ]) expect(() => parseStep(mark + line)).toThrow(`not a step: ${mark}${line}: `);
  expect(() => parseStep('refused')).toThrow('not a step: refused: missing operation');
});

test('host malformed replies fail before refusal comparison; state refusals keep their message', async () => {
  for (const marked of ['', 'refused ']) {
    const step = parseStep(`${marked}tap field`);
    await expect(step.run({ tap: async () => ({ error: 'bad argument', malformed: true }) })).rejects.toThrow(`not a step: ${marked}tap field: bad argument`);
    const refusal = new Error('hidden or inert\ndetail');
    expect(await step.run({ tap: async () => { throw refusal; } }).catch(e => e)).toBe(refusal);
  }
});

test('every existing step parses and keeps the driver call forms', async () => {
  const root = resolve(import.meta.dir, 'conformance');
  const files = readdirSync(root, { recursive: true }).filter(f => f.endsWith('.steps'));
  const record = async line => {
    const calls = [], s = Object.fromEntries(['tap', 'type', 'pointer', 'clock', 'resize', 'prefer', 'reload'].map(method => [method, async (...args) => { calls.push([method, args]); }]));
    s.state = async () => {
      calls.push(['state', []]);
      const [, path, , ...value] = line.split(/\s+/), parts = path.split('.'), state = {};
      parts.reduce((at, key, i) => at[key] = i === parts.length - 1 ? JSON.parse(value.join(' ')) : {}, state);
      return state;
    };
    await parseStep(line).run(s); return calls;
  };
  let count = 0;
  for (const file of files) for (const line of readFileSync(resolve(root, file), 'utf8').split('\n').map(l => l.trim()).filter(l => l && !l.startsWith('#'))) {
    expect((await record(line)).length, `${file}: ${line}`).toBeGreaterThan(0); count++;
  }
  expect(count).toBeGreaterThan(0);
  const forms = [
    ['tap a', [['tap', ['a']]]],
    ['menu a', [['tap', ['a', { contextmenu: true }]]]],
    ['type field several words', [['type', ['field', 'several words']]]],
    ['key field Enter down', [['type', ['field', { key: 'Enter', phase: 'down' }]]]],
    ['key field Enter for 500', [['type', ['field', { key: 'Enter', for: 500 }]]]],
    ['clock +100', [['clock', ['+100']]]],
    ['back a', [['tap', ['a', { history: -1 }]]]],
    ['wheel strip 20', [['tap', ['strip', { wheel: [0, 20] }]]]],
    ['wheel strip 20 10', [['tap', ['strip', { wheel: [10, 20] }]]]],
    ['into list row center', [['tap', ['list', { into: { key: 'row', block: 'center' } }]]]],
    ['pinch a 2', [['tap', ['a', { pinch: 2 }]]]],
    ['down a', [['tap', ['a', { down: true }]]]],
    ['up', [['pointer', ['up']]]],
    ['move 1 2', [['pointer', ['move', { dx: 1, dy: 2, ms: 200 }]]]],
    ['drag a to b', [['tap', ['a', { drag: { to: 'b', over: 200 } }]]]],
    ['drag a 1 2 300 hold 100', [['tap', ['a', { drag: { dx: 1, dy: 2, over: 300, hold: 100 } }]]]],
    ['drag a 1 2 300', [['tap', ['a', { down: true }]], ['pointer', ['move', { dx: 1, dy: 2, ms: 300 }]], ['pointer', ['up']]]],
    ['mediasession a play', [['tap', ['a', { mediaSession: 'play' }]]]],
    ['mediasession a seekto 5', [['tap', ['a', { mediaSession: 'seekto', seconds: 5 }]]]],
    ['prefer online true has-focus false', [['prefer', [{ online: 'true', 'has-focus': 'false' }]]]],
    ['resize 800x600', [['resize', [800, 600]]]],
  ];
  for (const [line, calls] of forms) expect(await record(line), line).toEqual(calls);
});

test('a storage checkpoint fails for missing or wrong state', async () => {
  const checkpoint = parseStep('expect background.done == 3');
  await checkpoint.run({ state: async () => ({ background: { done: 3 } }) });
  for (const state of [{}, { background: { done: 2 } }]) {
    await expect(checkpoint.run({ state: async () => state })).rejects.toThrow('expected 3');
  }
  await parseStep('expect slots.answer == "two  spaces"').run({ state: async () => ({ slots: { answer: 'two  spaces' } }) });
  for (const line of ['expect background.done == nope', 'expect slots.answer ==', 'expect done == 3', 'refused expect background.done == 3']) {
    expect(() => parseStep(line)).toThrow('not a step');
  }
});

test('a reload step restarts each executor on the store it had', async () => {
  const calls = [];
  await parseStep('reload').run({ reload: async () => { calls.push('reload'); } });
  expect(calls).toEqual(['reload']);
  for (const line of ['reload now', 'refused reload']) expect(() => parseStep(line)).toThrow('not a step');
  // The storage fixture holds the accounting to a fresh start after its restart.
  const steps = readFileSync(resolve(import.meta.dir, 'conformance/storage-background.steps'), 'utf8').split('\n');
  const after = steps.slice(steps.indexOf('reload') + 1);
  expect(steps).toContain('reload');
  for (const line of ['expect background.failed == 0', 'expect background.last == null']) expect(after).toContain(line);
});

test('cross-browser wheel steps request hover and wheel at one resolved point', async () => {
  const calls = [], session = who => ({ tap: async (...args) => { calls.push([who, ...args]); } });
  const step = parseStep('wheel strip 20 10', { hoverWheel: true });
  await step.run(session('chrome')); await step.run(session('other'));
  expect(calls).toEqual([
    ['chrome', 'strip', { wheel: [10, 20], hover: true }],
    ['other', 'strip', { wheel: [10, 20], hover: true }],
  ]);
  for (const line of ['wheel strip NaN', 'refused wheel strip 0 Infinity']) {
    expect(() => parseStep(line, { hoverWheel: true })).toThrow(`not a step: ${line}: `);
  }
});

test('conformance selects menus without treating the job count as an app', () => {
  const { options, named } = conformArgs(['menus', '--jobs', '1', '--build', '--strict']);
  expect(named).toEqual(['menus']);
  expect(options['--jobs']).toBe('1');
});

test('conformance consumes every value option between named apps', () => {
  const values = {
    '--wasm-root': 'wasm', '--out': 'report', '--steps': '0', '--browser': 'firefox',
    '--only': 'menus', '--label': 'probe', '--jobs': '2', '--js-from': 'earlier',
  };
  const { options, named } = conformArgs(['caltrain', ...Object.entries(values).flat(), 'menus']);
  expect(named).toEqual(['caltrain', 'menus']);
  expect(options).toEqual(values);
  const defaults = conformArgs([]);
  expect(defaults.named).toEqual([]);
  expect(defaults.options['--steps']).toBe(10);
  expect(defaults.options['--jobs']).toBeGreaterThanOrEqual(1);
  expect(defaults.options['--jobs']).toBeLessThanOrEqual(4);
});

test('URL comparison does not select its URLs or label as apps', () => {
  const { options, named } = conformArgs([
    '--urls', 'caltrain', 'http://localhost:8000', 'http://localhost:8001', '--label', 'preview',
  ]);
  expect(named).toEqual([]);
  expect(options['--label']).toBe('preview');
});

test("a { slots } Linux entry skips only the slots it names; a stop skips none", async () => {
  const { withoutSlots } = await import('./conform-linux.mjs');
  const slots = { geometry: 'caret width 0', sizes: 'caret 0×18', draft: 'hi' };
  expect(withoutSlots({ slots: ['sizes'], why: 'fonts' }, slots)).toEqual({ geometry: 'caret width 0', draft: 'hi' });
  expect(withoutSlots({ slots: ['nope'], why: 'x' }, slots)).toEqual(slots);
  expect(withoutSlots(['tap measure', 'why'], slots)).toEqual(slots);
  expect(withoutSlots(undefined, slots)).toEqual(slots);
  expect(withoutSlots({ slots: ['sizes'] }, undefined)).toBe(undefined);
});

test('a clock refused alike with Poisoned on every target is a step; one-sided, differing or other refusals fail', () => {
  const wasm = 'clock: Poisoned', js = 'clock: t=0 refused advance: Poisoned';
  expect(clockVerdict([['wasm', { error: wasm }], ['js', { error: js }]])).toEqual({ refused: 'Poisoned' });
  expect(clockVerdict([['wasm', { error: wasm }], ['js', { error: js }], ['linux', { error: 'clock: Poisoned' }]])).toEqual({ refused: 'Poisoned' });
  expect(clockVerdict([['wasm', { value: { clock: 5 } }], ['js', { value: { clock: 5 } }]])).toEqual({ answered: [{ clock: 5 }, { clock: 5 }] });
  expect(clockVerdict([['wasm', { error: wasm }], ['js', { value: { clock: 0 } }]]).failure).toBe('wasm refused «Poisoned», js answered');
  expect(clockVerdict([['wasm', { value: {} }], ['js', { error: js }]]).failure).toBe('wasm answered, js refused «Poisoned»');
  expect(clockVerdict([['wasm', { error: wasm }], ['js', { error: 'clock: t=0 refused advance: NonFiniteClock (NaN)' }]]).failure)
    .toBe('wasm refused «Poisoned», js refused «NonFiniteClock (NaN)»');
  expect(clockVerdict([['wasm', { error: 'clock: TimerFireLimit' }], ['js', { error: 'clock: TimerFireLimit' }]]).failure)
    .toBe('wasm refused «TimerFireLimit», js refused «TimerFireLimit»');
});

test("the drive's clock step holds Linux to the pages' Poisoned refusal, and asks it nothing when they answer", async () => {
  const pair = (left, right) => Promise.all([left(), right()]);
  const asked = [], session = (who, answer) => ({ clock: async arg => { asked.push([who, arg]); return answer(); } });
  const refuse = text => () => { throw new Error(text); };
  const pages = [['wasm', session('wasm', refuse('clock: clock: Poisoned'))], ['js', session('js', refuse('clock: clock: t=0 refused advance: Poisoned'))]];
  expect(await clockOnAll('settle', pages, session('linux', refuse('clock: clock: Poisoned')), pair))
    .toEqual({ refused: 'Poisoned', compared: ['wasm', 'js', 'linux'] });
  expect(await clockOnAll('settle', pages, null, pair)).toEqual({ refused: 'Poisoned', compared: ['wasm', 'js'] });
  expect(await clockOnAll('+60000', pages, session('linux', () => ({ clock: 50, settled: true })), pair))
    .toEqual({ refused: 'Poisoned', linux: 'wasm refused «Poisoned», js refused «Poisoned», linux answered' });
  expect((await clockOnAll('settle', pages, session('linux', refuse('clock: clock: TimerFireLimit { limit: 4096 }')), pair)).linux)
    .toBe('wasm refused «Poisoned», js refused «Poisoned», linux refused «TimerFireLimit { limit: 4096 }»');
  asked.length = 0;
  const answered = [['wasm', session('wasm', () => ({ clock: 5 }))], ['js', session('js', () => ({ clock: 5 }))]];
  expect(await clockOnAll('settle', answered, session('linux', () => ({ clock: 5 })), pair)).toEqual({ answered: [{ clock: 5 }, { clock: 5 }], compared: ['wasm', 'js'] });
  expect(asked.map(([who]) => who)).toEqual(['wasm', 'js']);
  const malformed = Object.assign(new Error('bad'), { malformed: true });
  await expect(clockOnAll('settle', [pages[0], ['js', session('js', () => { throw malformed; })]], null, pair)).rejects.toBe(malformed);
});

test('the wasm reference marker covers exactly photo-editor, recorder and markdown, with exactly its reason', async () => {
  const { WASM_UNBOUND, WASM_UNBOUND_REASON } = await import('./conform-marked.mjs');
  // Interim (theme2 + the lead, 2026-10-30): the landing fix removes it. A fourth app or another reason fails here.
  expect(WASM_UNBOUND_REASON).toBe('reference does not bind packages (LLP 1071 retirement)');
  expect(Object.keys(WASM_UNBOUND).sort()).toEqual(['markdown', 'photo-editor', 'recorder']);
  for (const reason of Object.values(WASM_UNBOUND)) expect(reason).toBe('reference does not bind packages (LLP 1071 retirement)');
  expect(Object.isFrozen(WASM_UNBOUND)).toBe(true);
  expect(Object.getPrototypeOf(WASM_UNBOUND)).toBe(Object.prototype);
});

test("the marker records a marked target's differences, counted; other targets' differences and every non-difference fail", async () => {
  const { WASM_UNBOUND_REASON, markUnbound } = await import('./conform-marked.mjs');
  const report = { failures: [
    { target: 'recorder', step: 'boot', what: 'state slots.voice: wasm null js "x"', difference: true },
    { target: 'recorder', step: 'boot', what: 'tree #3: wasm «a» js «b»', difference: true },
    { target: 'recorder', step: 'tap record', what: 'screenshot: 4.00% of pixels differ', difference: true },
    { target: 'recorder', step: 'wasm-open', what: 'refused: stale dist' },
    { target: 'recorder', step: 'tap stop', what: 'js: hidden or inert' },
    { target: 'recorder', step: 'tap stop state', what: 'TypeError: crash' },
    { target: 'markdown', step: 'wasm-build', what: 'error[E0425]' },
    { target: 'photo-editor', step: 'tests', what: 'wasm test x FAIL' },
    { target: 'caltrain', step: 'boot', what: 'tree #0: wasm «a» js «b»', difference: true },
    { target: 'synthetic-recorder', step: 'boot', what: 'state x', difference: true },
    { target: 'toString', step: 'boot', what: 'state x', difference: true },
  ] };
  markUnbound(report);
  expect(report.marked).toEqual({ recorder: { reason: WASM_UNBOUND_REASON, differences: 3, steps: ['boot', 'tap record'], lines: [
    'boot: state slots.voice: wasm null js "x"', 'boot: tree #3: wasm «a» js «b»', 'tap record: screenshot: 4.00% of pixels differ'] } });
  expect(report.failures.map(f => `${f.target} ${f.step}`)).toEqual(['recorder wasm-open', 'recorder tap stop', 'recorder tap stop state',
    'markdown wasm-build', 'photo-editor tests', 'caltrain boot', 'synthetic-recorder boot', 'toString boot']);
  const clean = markUnbound({ failures: [] });
  expect(clean).toEqual({ failures: [], marked: {} });
});

test('every wasm-vs-JS comparison site records through differ, which alone tags a difference; Linux and refusals do not', () => {
  const source = readFileSync(resolve(import.meta.dir, 'conform.mjs'), 'utf8');
  expect(source).toContain('const differ = (step, what, field = null) => fail(step, what, field, !crossBrowser);');
  const sites = source.split('\n').filter(l => /\bdiffer\(step,/.test(l));
  expect(sites.length).toBe(8);
  for (const [i, mark] of ["differ(step, 'state ' + x)", 'differ(step, x, crossBrowser ? `tree.${i}` : null)', 'differ(step, x.what, crossBrowser ? x.field : null)',
    'op.forEach(x => differ(step, x))', 'differ(step, `screenshot: ', 'differ(step, `bound refusals: ', 'differ(step, `sound lines: ', 'differ(step, `storage lines: '].entries()) expect(sites[i], mark).toContain(mark);
  // No other call passes `fail`'s difference flag, and Linux's comparisons fail as they are.
  expect(source.split('\n').filter(l => /\bfail\([^\n]*,\s*(?:true|!crossBrowser)\)/.test(l) && !l.includes('const differ ='))).toEqual([]);
  expect(source).toContain("o.forEach(x => fail(step, 'linux ' + x));");
  expect(source).toContain('ol.forEach(x => fail(step, x.what));');
  expect(source).toContain('referenceTapRefused({ target: t.name, step: `tap ${id}`');
});

test("a marked target's refused reference tap stays a failure, never marked; elsewhere it is skipped as before", async () => {
  const { markUnbound, referenceTapRefused } = await import('./conform-marked.mjs');
  const report = { failures: [], steps: [] };
  // The drive's own `fail` (conform.mjs `target`): no field, no difference flag.
  const failFor = target => (step, what, field = null, difference = false) => { report.failures.push({ target, step, what, ...(field == null ? {} : { field }), ...(difference ? { difference: true } : {}) }); };
  referenceTapRefused({ target: 'recorder', step: 'tap record', why: 'wasm: tap record: disabled', crossBrowser: undefined, fail: failFor('recorder'), report });
  referenceTapRefused({ target: 'caltrain', step: 'tap open', why: 'wasm: tap open: hidden', crossBrowser: undefined, fail: failFor('caltrain'), report });
  referenceTapRefused({ target: 'recorder', step: 'tap stop', why: 'chrome: tap stop: disabled', crossBrowser: 'firefox', fail: failFor('recorder'), report });
  markUnbound(report);
  expect(report.marked).toEqual({});
  expect(report.failures).toEqual([{ target: 'recorder', step: 'tap record', what: 'wasm: tap record: disabled (the reference refused the tap, so the JS page never took it)' }]);
  expect(report.steps).toEqual([{ target: 'caltrain', step: 'tap open', skipped: 'wasm: tap open: hidden' }, { target: 'recorder', step: 'tap stop', skipped: 'chrome: tap stop: disabled' }]);
});

test('the marker marks every comparison kind on a marked target and leaves Linux differences failing', async () => {
  const { markUnbound } = await import('./conform-marked.mjs');
  const d = (step, what) => ({ target: 'markdown', step, what, difference: true });
  const report = { failures: [d('boot', 'layout editor.h: wasm 10.00 js 12.00 (delta 2.00 px, tolerance 0.5 px)'), d('boot', 'paint.0.1: wasm "auto" js "isolate"'),
    d('tap a', 'bound refusals: wasm «—» js «Trap(IterationLimit { pc: 3 })»'), d('tap a', 'sound lines: wasm «—» js «t=0 sound x»'),
    { target: 'markdown', step: 'tap a', what: 'linux state slots.x: wasm 1 linux 2' }, { target: 'markdown', step: 'tap a', what: 'layout editor.h: wasm 10.00 kernel 12.00' }] };
  markUnbound(report);
  expect(report.marked.markdown.differences).toBe(4);
  expect(report.marked.markdown.steps).toEqual(['boot', 'tap a']);
  expect(report.failures.map(f => f.what)).toEqual(['linux state slots.x: wasm 1 linux 2', 'layout editor.h: wasm 10.00 kernel 12.00']);
});

test('bound comparisons include the evaluation and words while joining view and site identities', () => {
  const source = comparisonSource.slice(comparisonSource.indexOf('const BOUND ='), comparisonSource.indexOf('const boundReasons ='));
  const start = comparisonSource.indexOf('const boundReasons =');
  const end = comparisonSource.indexOf('}).filter((r, i, all) => r !== all[i - 1]);', start) + '}).filter((r, i, all) => r !== all[i - 1]);'.length;
  const reasons = new Function(source + comparisonSource.slice(start, end) + '; return boundReasons;')();
  const suffix = ': past the 65,536 list steps one evaluation may take';
  expect(reasons([`poisoned: Instance(Trap(IterationLimit { pc: 17 })) in view 12 (site 58) each subject${suffix}`]))
    .toEqual(reasons([`poisoned: Instance(Trap(IterationLimit { pc: 17 })) in site 58 each subject${suffix}`]));
  expect(reasons([`refused: Trap(IterationLimit { pc: 17 }) in derive "a"${suffix}`]))
    .not.toEqual(reasons([`refused: Trap(IterationLimit { pc: 17 }) in derive "b"${suffix}`]));
});
