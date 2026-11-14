import { test, expect } from 'bun:test';
import { conformArgs } from './conform-args.mjs';
import { clockOnAll, clockVerdict, failedTests, parseStep } from './conform-steps.mjs';
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
async function fixtureDrive(browser, {
  refuse = null, broken = null, unavailable = null, poison = null, screenshotError = null,
  appTests = false, settleAfterRefusal = false, knownTree = false, evaluate = async () => null, shot = async () => null,
  script = 'tap scripted\nrefused tap refused\nreload\nwheel auto 12\nclock +5',
} = {}) {
  const events = [], sessions = [], shots = [];
  const report = { targets: { fixture: { jsBuild: true } }, steps: [], failures: [], known: [] };
  const comparisons = new Function(comparisonSource.slice(comparisonSource.indexOf('async function comparisonTree('), comparisonSource.indexOf('function diffPng('))
    + '; return { diffJSON, diffJSONFields, diffLists, diffLayout, norm };')();
  const deps = {
    ...comparisons, crossBrowser: browser, crossBrowsers: browser?.split(',') ?? [],
    readyBrowsers: browser?.split(',') ?? [], maxSteps: 3,
    STATE_KEYS: ['slots'], LINUX_APART: {}, LINUX_OPS: [], linuxFor: () => null, linuxRef: false,
    root: '/checkout', here: '/checkout/host/web-js', resolve, parseStep, clockOnAll, failedTests,
    resolveApp: () => ({ dir: '/fixture' }),
    existsSync: path => path.endsWith('.steps') || (appTests && path.endsWith('app.test.contract')),
    readFileSync: path => path.endsWith('.steps') ? script : '',
    reloadWeb: async session => { events.push([session.browser, 'reload']); },
    referenceTapRefused: () => { throw new Error('unexpected reference refusal'); },
    addSession() {}, storageLine: x => x,
    diffPng: () => 0,
    journal: async session => {
      events.push([session.browser, 'journal']);
      const sounds = session.pending.splice(0);
      return { bounds: [], sounds, storage: [] };
    },
    comparisonTree: session => session.tree(),
    comparisonLayout: async session => [{ nodes: [{ id: 1, testId: 'auto', x: 0, y: 0, w: session.browser === 'webkit' && !knownTree ? 20 : 10, h: 10 }] }, [0, 0]],
    serve: async () => ({ url: '/tests', close() {} }),
    spawn: (_command, args) => {
      const browser = args[args.indexOf('--browser') + 1], handlers = {};
      events.push([browser, 'tests']);
      const child = {
        stdout: { on: (_event, fn) => { handlers.stdout = fn; } },
        stderr: { on() {} },
        on: (event, fn) => { handlers[event] = fn; return child; },
      };
      queueMicrotask(() => {
        handlers.stdout(`test fixture ${browser === 'webkit' ? 'FAIL\nwrong value\n0 passed, 1 failed' : 'PASS\n1 passed, 0 failed'}\n`);
        handlers.exit(browser === 'webkit' ? 1 : 0);
      });
      return child;
    },
    open: async options => {
      const { browser } = options;
      events.push([browser, 'open', options]);
      if (browser === unavailable) throw new Error('engine unavailable');
      const index = sessions.length;
      const session = {
        browser, value: 0, pending: ['boot'], carrier: { evaluate: source => evaluate(source, index) },
        state: async () => {
          events.push([browser, 'state']);
          return { slots: { value: session.value } };
        },
        tree: async () => {
          events.push([browser, 'tree']);
          if (browser === broken) throw new Error('tree unavailable');
          const nodes = [{ id: 1, type: 'Control', props: { testId: 'auto', text: String(session.value) }, handlers: ['press'] }];
          if (browser !== 'chrome') nodes.unshift({ id: 2, type: 'Control', props: { testId: 'decoy' }, handlers: ['press'] });
          return { nodes };
        },
        tap: async (id, options) => {
          events.push([browser, 'tap', id, options]);
          if (id === 'refused') {
            session.refused = true;
            throw new Error(`tap refused: ${settleAfterRefusal && browser === 'firefox' ? 'unavailable' : 'disabled'}`);
          }
          if (browser === refuse && id === 'scripted') throw new Error('tap scripted: unavailable');
          session.value++;
          session.pending.push(id);
        },
        clock: async arg => {
          events.push([browser, 'clock', arg]);
          if (poison?.includes(browser) || (settleAfterRefusal && session.refused && browser === 'firefox')) throw new Error('clock: Poisoned');
          return {};
        },
        screenshot: async path => {
          shots.push([browser, path, await shot(index)]);
          if (browser === screenshotError) throw new Error('capture unavailable');
        },
        close: async () => { events.push([browser, 'close']); },
      };
      sessions.push(session);
      return session;
    },
  };
  const source = comparisonSource.slice(comparisonSource.indexOf('async function drive('), comparisonSource.indexOf('// ---------------------------------------------------------------- activation'));
  const drive = new Function(...Object.keys(deps), source + '; return drive;')(...Object.values(deps));
  const failureSource = comparisonSource.slice(comparisonSource.indexOf('  const fail =', comparisonSource.indexOf('async function target(')), comparisonSource.indexOf('  const dir = resolve(out, t.name)'));
  const fail = new Function('crossBrowsers', 'crossBrowser', 'knownFor', 'report', 't', failureSource + '; return fail;')(
    deps.crossBrowsers, browser, (_app, _step, field, _what, who) => knownTree && who === 'webkit' && field.startsWith('tree.') ? { reason: 'fixture tree difference' } : null,
    report, { name: 'fixture' },
  );
  await drive({ name: 'fixture', app: 'fixture', ...(appTests ? { dist: '/dist' } : { contract: '/fixture.contract' }) }, report, fail, '/out/fixture', { close() {} }, { close() {} });
  return { report, events, sessions, shots };
}

test('Firefox and WebKit share each live Chrome step and retain their separate findings', async () => {
  const singles = await Promise.all(['firefox', 'webkit'].map(browser => fixtureDrive(browser)));
  const combined = await fixtureDrive('firefox,webkit');
  expect(combined.events.filter(([, op]) => op === 'open').map(([browser]) => browser).sort()).toEqual(['chrome', 'firefox', 'webkit']);
  const operations = (run, browser) => run.events.filter(([who]) => who === browser).map(([, ...op]) => op);
  expect(operations(combined, 'chrome')).toEqual(operations(singles[0], 'chrome'));
  for (const [i, browser] of ['firefox', 'webkit'].entries()) {
    expect(operations(combined, browser)).toEqual(operations(singles[i], browser));
    const failures = run => run.report.failures.filter(f => !f.browser || f.browser === browser).map(({ browser: _browser, ...f }) => f);
    expect(failures(combined)).toEqual(failures(singles[i]));
    expect(combined.report.steps.filter(s => s.browser === browser).map(s => s.step)).toContain('tap auto');
    expect(combined.sessions.find(s => s.browser === browser).value).toBe(3);
  }
  expect(combined.shots.some(([browser]) => browser === 'webkit')).toBe(true);
  expect(combined.shots.some(([browser]) => browser === 'chrome')).toBe(true);
  expect(combined.report.failures.some(f => /sound lines/.test(f.what))).toBe(false);
  expect(combined.events.some(([, op, id]) => op === 'tap' && id === 'decoy')).toBe(false);
  expect(new Set(combined.shots.map(([, path]) => path)).size).toBe(combined.shots.length);
});

test('the wasm and JS drive still delivers every step to its two Chrome pages', async () => {
  const run = await fixtureDrive(null);
  expect(run.sessions.map(s => s.browser)).toEqual(['chrome', 'chrome']);
  expect(run.sessions.map(s => s.value)).toEqual([3, 3]);
  expect(run.report.failures).toEqual([]);
  expect(run.report.steps.map(s => s.step)).toEqual(['boot', 'tap scripted', 'refused tap refused', 'reload', 'wheel auto 12', 'clock +5', 'tap auto', 'clock +60000']);
  expect(run.report.steps.every(s => !s.browser && s.differences === 0)).toBe(true);
});

test('conformance screenshots pin only auto images to one filter, after both pages agree on each image\'s', async () => {
  const browser = await suiteBrowser();
  const markup = first => `<img style="image-rendering:${first}"><img style="image-rendering:crisp-edges"><div style="image-rendering:pixelated"><img></div><img>`;
  const modes = page => page.locator('img').evaluateAll(images => images.map(image => [getComputedStyle(image).imageRendering, image.getAttribute('style')]));
  try {
    const agreeing = [await browser.newPage(), await browser.newPage()];
    for (const page of agreeing) await page.setContent(markup('auto'));
    const run = await fixtureDrive(null, { evaluate: (source, i) => agreeing[i].evaluate(source), shot: i => modes(agreeing[i]) });
    const captured = [['pixelated', 'image-rendering:auto'], ['crisp-edges', 'image-rendering:crisp-edges'], ['pixelated', null], ['pixelated', null]];
    expect(run.shots.length).toBeGreaterThan(0);
    for (const [, , seen] of run.shots) expect(seen.map(([mode]) => mode)).toEqual(captured.map(([mode]) => mode));
    expect(run.report.failures).toEqual([]);
    for (const page of agreeing) {
      expect(await modes(page)).toEqual([['auto', 'image-rendering:auto'], ['crisp-edges', 'image-rendering:crisp-edges'], ['pixelated', null], ['auto', null]]);
    }
    const apart = [await browser.newPage(), await browser.newPage()];
    await apart[0].setContent(markup('auto')); await apart[1].setContent(markup('crisp-edges'));
    const disagreeing = await fixtureDrive(null, { evaluate: (source, i) => apart[i].evaluate(source) });
    expect(disagreeing.report.failures.some(f => /image-rendering/.test(f.what))).toBe(true);
  } finally { await browser.close(); }
});

test('one engine can refuse, fail to open or lose its tree while the other finishes', async () => {
  for (const options of [{ refuse: 'firefox' }, { unavailable: 'firefox' }, { broken: 'firefox' }]) {
    const run = await fixtureDrive('firefox,webkit', options);
    expect(run.report.failures.some(f => f.browser === 'firefox')).toBe(true);
    expect(run.report.steps.some(s => s.browser === 'webkit' && s.step === 'clock +60000')).toBe(true);
    expect(run.sessions.find(s => s.browser === 'webkit').value).toBe(3);
    expect(run.events.filter(([, op]) => op === 'close')).toHaveLength(run.sessions.length);
  }
});

test('clock refusals compare separately and unsupported touches never reach Chrome', async () => {
  const refused = await fixtureDrive('firefox,webkit', { poison: ['chrome', 'webkit'], script: 'down missing\nrefused drag missing 0 10\nclock +5' });
  expect(refused.report.failures.filter(f => f.browser === 'firefox').some(f => /clock settle: chrome refused.*firefox answered/.test(f.what))).toBe(true);
  expect(refused.report.steps.some(s => s.browser === 'webkit' && s.refused === 'Poisoned on chrome, webkit')).toBe(true);
  expect(refused.report.steps.filter(s => s.skipped).map(s => s.skipped.split(':')[0])).toEqual(['webkit', 'webkit']);
  expect(refused.events.some(([, op, id]) => op === 'tap' && id === 'missing')).toBe(false);
  const skipped = await fixtureDrive('firefox,webkit', { script: 'down missing' });
  expect(skipped.report.steps.filter(s => s.skipped).map(s => s.skipped.split(':')[0])).toEqual(['firefox', 'webkit']);
});

test('peer exceptions retain the precise tree and clock step for lane failure identities', async () => {
  const broken = await fixtureDrive('firefox,webkit', { broken: 'firefox' });
  expect(broken.report.failures.filter(f => f.browser === 'firefox').map(f => f.step)).toEqual(['boot tree']);
  const boot = await fixtureDrive('firefox,webkit', { poison: ['firefox'] });
  expect(boot.report.failures.filter(f => f.browser === 'firefox').map(f => f.step)).toEqual(['boot clock settle']);
  const refused = await fixtureDrive('firefox,webkit', { script: 'refused tap refused', settleAfterRefusal: true });
  const failures = refused.report.failures.filter(f => f.browser === 'firefox' && f.step.startsWith('refused tap refused'));
  expect(failures.map(f => f.step)).toEqual(['refused tap refused', 'refused tap refused clock settle']);
  const { failures: laneFailures } = await import('../../scripts/async.mjs');
  const log = failures.map(f => `FAIL ${f.browser} ${f.target} ${f.step}: ${f.what}`).join('\n');
  expect(laneFailures('conform-browsers', log, 1)).toEqual([
    'conform-browsers: firefox fixture refused tap refused',
    'conform-browsers: firefox fixture refused tap refused clock settle',
  ]);
});

test('a failed oracle screenshot preserves both engine captures and subsequent comparisons', async () => {
  const run = await fixtureDrive('firefox,webkit', { screenshotError: 'chrome' });
  for (const browser of ['firefox', 'webkit']) {
    expect(run.shots.some(([who]) => who === browser)).toBe(true);
    expect(run.report.failures.some(f => f.browser === browser && f.what === 'chrome screenshot: capture unavailable')).toBe(true);
    expect(run.report.steps.some(s => s.browser === browser && s.step === 'clock +60000')).toBe(true);
  }
});

test('known differences belong to the engine that declared them', () => {
  const source = comparisonSource.slice(comparisonSource.indexOf('const knownFor ='), comparisonSource.indexOf("const only = options['--only'];"));
  const knownFor = new Function('crossBrowsers', 'readFileSync', 'resolve', 'here', source + '; return knownFor;')(
    ['firefox', 'webkit'], readFileSync, resolve, import.meta.dir,
  );
  const what = 'tree #0: chrome «0|Control|button|click|||press|focused» webkit «0|Control|button|click|||press|»';
  expect(knownFor('fixture', 'tap button', 'tree.0', what, 'webkit')).toBeDefined();
  expect(knownFor('fixture', 'tap button', 'tree.0', what, 'firefox')).toBeUndefined();
});

test('known tree differences remain unequal checkpoints with screenshots and no failures', async () => {
  const run = await fixtureDrive('webkit', { knownTree: true, script: '' });
  expect(run.report.failures).toEqual([]);
  expect(run.report.known.length).toBeGreaterThan(0);
  expect(run.report.steps.every(s => s.differences > 0)).toBe(true);
  expect(run.shots.filter(([browser]) => browser === 'chrome')).toHaveLength(run.report.steps.length);
  expect(run.shots.filter(([browser]) => browser === 'webkit')).toHaveLength(run.report.steps.length);
});

test('authored tests run Chrome once and report each engine even after its drive diverged', async () => {
  const run = await fixtureDrive('firefox,webkit', { appTests: true, refuse: 'firefox' });
  expect(run.events.filter(([, op]) => op === 'tests').map(([browser]) => browser)).toEqual(['chrome', 'firefox', 'webkit']);
  expect(run.report.targets.fixture.tests).toEqual({ chrome: '1 passed, 0 failed', firefox: '1 passed, 0 failed', webkit: '0 passed, 1 failed' });
  const failures = run.report.failures.filter(f => f.step === 'tests');
  expect(failures.every(f => f.browser === 'webkit')).toBe(true);
  expect(failures.some(f => f.what.includes('wrong value'))).toBe(true);
});

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
  const carry = async (sourceTree, crossBrowser = 'firefox', readyBrowsers = crossBrowser.split(',')) => {
    const deps = {
      out: '/fixture-run', root: '/checkout', sdir: '/checkout/host/web-js/conformance', sourceTree,
      resolve, dirname, basename, mkdirSync() {}, rmSync() {}, cpSync() {}, symlinkSync() {}, appendFileSync() {},
      existsSync: path => files.has(path) || path.endsWith('/report.json'), writeFileSync: write,
      readFileSync: path => path.endsWith('/app.ts') ? 'export const appId = "fieldnotes";\nexport const grants = {};'
        : path.endsWith('/app.json') ? '{}' : path.endsWith('/report.json') ? '{"targets":{},"steps":[],"failures":[],"known":[]}' : '',
      spawnSync: () => ({ status: 0, stdout: '', stderr: '' }), process: { env: {}, execPath: process.execPath },
      crossBrowser, readyBrowsers, jsFrom: '/previous', linuxRef: false, argv: [],
      spawned: async (_command, args, { env }) => {
        expect(args[args.indexOf('--browser') + 1]).toBe(readyBrowsers.join(','));
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
  await carry(before, 'firefox,webkit');
  await carry(before, 'firefox,webkit', ['webkit']);
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
    for (const engine of ['firefox', 'webkit', 'firefox,webkit']) {
      const engines = engine.split(',');
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
        mock.module('playwright-core', () => ({ ${engines.map(e => `${e}: { launchServer: async () => { throw new Error('fixture launch failure'); } }`).join(', ')} }));
        process.argv = [process.execPath, 'conform.mjs', '--synthetic', '--only', 'document', '--browser', ${JSON.stringify(engine)},
          '--build-js', '--build', '--strict', '--out', ${JSON.stringify(destination)}, '--wasm-root', ${JSON.stringify(resolve(out, 'wasm'))}];
        await import(${JSON.stringify(resolve(import.meta.dir, 'conform.mjs'))});
      `], { encoding: 'utf8', timeout: 15000 });
      expect(run.status, run.stderr).toBe(1);
      const report = JSON.parse(readFileSync(resolve(destination, 'report.json'), 'utf8'));
      expect(report.failures).toEqual(engines.map(browser => ({ target: 'engine', step: 'launch',
        ...(engines.length > 1 ? { browser } : {}), what: `web carrier unavailable: ${browser}: fixture launch failure` })));
      expect(existsSync(compiler)).toBe(false);
      expect(report.targets).toEqual({});
      expect(report.steps).toEqual([]);
    }
  } finally { rmSync(out, { recursive: true, force: true }); }
}, 30000);

for (const [failed, at] of [['firefox', 'preflight'], ['webkit', 'preflight'], ['firefox', 'open']]) {
  test(`a ${failed} ${at} failure leaves the other engine's drive and report intact`, () => {
    const out = mkdtempSync(resolve(tmpdir(), 'exact-conform-survivor-'));
    const survivor = failed === 'firefox' ? 'webkit' : 'firefox';
    try {
      const run = spawnSync(process.execPath, ['-e', `
        import { mock } from 'bun:test';
        import { appendFileSync } from 'node:fs';
        mock.module('playwright-core', () => Object.fromEntries(['firefox', 'webkit'].map(browser => [browser, {
          launchServer: async () => {
            if (browser === ${JSON.stringify(failed)} && ${JSON.stringify(at)} === 'preflight') throw new Error('fixture launch failure');
            return { close: async () => {} };
          },
        }])));
        mock.module(${JSON.stringify(resolve(import.meta.dir, '../../scripts/agent.mjs'))}, () => ({ open: async ({ browser, lineHeight }) => {
          appendFileSync(${JSON.stringify(resolve(out, 'opens.jsonl'))}, JSON.stringify({ browser, lineHeight }) + '\\n');
          if (browser === ${JSON.stringify(failed)}) throw new Error('fixture open failure');
          return {
            state: async () => ({ slots: { value: browser === 'chrome' ? 0 : 1 } }),
            tree: async () => ({ nodes: [{ id: 0, type: 'Text', props: { testId: 'value', text: 'value' } }] }),
            layout: async () => ({ nodes: [{ id: 0, testId: 'value', x: browser === 'chrome' ? 0 : 2.5, y: 0, w: 10, h: 10 }] }),
            logs: async () => ({ lines: [] }), clock: async () => ({}),
            screenshot: async () => {}, close: async () => {},
            carrier: { evaluate: async code => code.includes('scroll:') ? { scroll: [0, 0], fixed: [] }
              : { policy: 'eager', modules: 0, preload: true, paint: 1, boot: 2, adopted: true } },
          };
        } }));
        process.argv = [process.execPath, 'conform.mjs', '--urls', 'variable-type', 'http://127.0.0.1:1/', 'http://127.0.0.1:1/',
          '--browser', 'firefox,webkit', '--build-js', '--strict', '--out', ${JSON.stringify(out)}, '--wasm-root', ${JSON.stringify(resolve(out, 'wasm'))}];
        await import(${JSON.stringify(resolve(import.meta.dir, 'conform.mjs'))});
      `], { encoding: 'utf8', timeout: 15000 });
      expect(run.status, run.stderr).toBe(1);
      expect(existsSync(resolve(out, 'report.json')), run.stderr).toBe(true);
      const report = JSON.parse(readFileSync(resolve(out, 'report.json'), 'utf8'));
      const failedEntries = report.failures.filter(f => f.browser === failed);
      expect(failedEntries).toHaveLength(at === 'preflight' ? 1 : 2);
      expect(failedEntries[0].step).toBe(at === 'preflight' ? 'launch' : `${failed}-open`);
      expect(report.failures.some(f => f.browser === survivor && f.field === 'state.slots.value')).toBe(true);
      expect(report.steps.some(s => s.browser === survivor && s.step === 'clock +60000')).toBe(true);
      const opens = readFileSync(resolve(out, 'opens.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
      expect(opens.filter(o => o.browser === 'chrome' && o.lineHeight === '1.2')).toHaveLength(1);
      if (at === 'preflight') expect(opens.some(o => o.browser === failed)).toBe(false);
      const markdown = readFileSync(resolve(out, 'report.md'), 'utf8');
      const row = browser => markdown.split('\n').find(line => line.startsWith(`| variable-type-urls / ${browser} |`));
      expect(row(failed)).toEndWith('| 0.000 px |');
      expect(row(survivor)).toEndWith('| 2.500 px |');
    } finally { rmSync(out, { recursive: true, force: true }); }
  });
}

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
  expect(sites.length).toBe(9);
  for (const [i, mark] of ["differ(step, 'state ' + x)", 'differ(step, x, crossBrowser ? `tree.${i}` : null)', 'differ(step, x.what, crossBrowser ? x.field : null)',
    'op.forEach(x => differ(step, x))', 'differ(step, `image-rendering: ', 'differ(step, `screenshot: ', 'differ(step, `bound refusals: ', 'differ(step, `sound lines: ', 'differ(step, `storage lines: '].entries()) expect(sites[i], mark).toContain(mark);
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
  for (const [kind, suffix] of [
    ['CallLimit', ': past the 65,536 derive calls one evaluation may take'],
    ['CallDepth', ': past 64 derive call levels'],
  ]) {
    const reason = `Trap(${kind} { pc: 19 })${suffix}`;
    expect(reasons([`site 0: text trapped ${reason}; unset`])).toEqual([reason]);
  }
  const suffix = ': past the 65,536 list steps one evaluation may take';
  expect(reasons([`poisoned: Instance(Trap(IterationLimit { pc: 17 })) in view 12 (site 58) each subject${suffix}`]))
    .toEqual(reasons([`poisoned: Instance(Trap(IterationLimit { pc: 17 })) in site 58 each subject${suffix}`]));
  expect(reasons([`refused: Trap(IterationLimit { pc: 17 }) in derive "a"${suffix}`]))
    .not.toEqual(reasons([`refused: Trap(IterationLimit { pc: 17 }) in derive "b"${suffix}`]));
});

test('a failed test\'s detail leaves out a long test\'s progress and the property count', () => {
  const output = ['test "a": FAIL', '  a: line 3: expected 1', 'still running test "b": 30s, at line 9 of 12 (drag)', 'properties on web: 4 always line checks', 'test "b": FAIL', '  b: line 9: no', '1 passed, 2 failed'].join('\n');
  expect(failedTests(output)).toEqual(['test "a": FAIL | a: line 3: expected 1', 'test "b": FAIL | b: line 9: no']);
});
