import { test, expect } from 'bun:test';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { webPackageBindingChanged } from '../../js/bake/src/package-config.mjs';
import { buildPackageViews } from './build.mjs';
import { hostFixtureFile } from './tests/glue-fixture.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

test.each([false, true])('two placed views share the configured implementation, admitted files and lifecycle of their wasm session (production %s)', async production => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-wasm-swatch-')), stage = join(dir, 'dist');
  const pkg = join(dir, 'node_modules/@fixture/swatch');
  const placed = [{ tag: 'swatch-box', extension: 'swatch', package: '@fixture/swatch', displayed: true, binds: 'tint' }];
  const canary = 'swatch-wasm-development-02', previousKey = process.env.EXACT_WASM_SWATCH_KEY;
  let server, browser;
  try {
    mkdirSync(stage);
    cpSync(resolve(import.meta.dir, '../../packages/fixtures/swatch'), pkg, { recursive: true });
    process.env.EXACT_WASM_SWATCH_KEY = 'base';
    writeFileSync(join(dir, 'app.json'), JSON.stringify({
      packages: { '@fixture/swatch': { web: { label: 'tiles', apiKey: { env: 'EXACT_WASM_SWATCH_KEY' } } } },
      development: { packages: { '@fixture/swatch': { web: { label: canary } } } },
    }));
    mkdirSync(join(dir, 'config'));
    renameSync(join(dir, 'app.json'), join(dir, 'config/manifest.json'));
    symlinkSync('config/manifest.json', join(dir, 'app.json'));
    mkdirSync(join(dir, '.shells'));
    writeFileSync(join(dir, '.shells/app.json'), readFileSync(join(dir, 'app.json')));
    writeFileSync(join(pkg, 'web/index.js'), readFileSync(join(pkg, 'web/index.js'), 'utf8')
      .replace('export function implementation', 'function swatch') + `
      import { color } from './palette';
      import manifest from '../../../../config/manifest.json';
      import resolved from '../../../../.shells/app.json';
      export function implementation(context) {
        const impl = swatch(context);
        window.instances.push({ impl, context, color, manifest, resolved });
        return impl;
      }`);
    mkdirSync(join(pkg, 'web/palette'));
    writeFileSync(join(pkg, 'web/palette/index.js'), "export const color = 'red';");
    expect(await buildPackageViews(dir, [], [], stage, false)).toBeNull();
    expect(await buildPackageViews(dir, placed, ['swatch-box'], stage, false)).toBeNull();
    expect(existsSync(join(stage, 'packages'))).toBe(false);
    const receipt = await buildPackageViews(dir, placed, [], stage, production);
    const chunks = readdirSync(join(stage, 'packages')).map(file => readFileSync(join(stage, 'packages', file), 'utf8'));
    expect(chunks.some(code => code.includes(canary))).toBe(!production);
    expect(receipt.files).toContain(realpathSync(join(pkg, 'web/index.js')));
    expect(webPackageBindingChanged(dir, receipt)).toBe(false);
    writeFileSync(join(pkg, 'web/palette.js'), "export const color = 'blue';");
    expect(webPackageBindingChanged(dir, receipt)).toBe(true);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response('<main id="exact-root"></main>', { headers: { 'content-type': 'text/html' } });
      if (path.startsWith('packages/')) return new Response(Bun.file(join(stage, path)));
      if (!['native-glue.js', 'navigation.js', 'storage-fs.js', 'grant-admission.js', 'storage-environment.js', 'wakelock-glue.js'].includes(path)) return new Response(null, { status: 404 });
      return new Response(hostFixtureFile(join(import.meta.dir, path)), { headers: { 'content-type': 'text/javascript' } });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.exact = { ready: Promise.resolve(), compat: { inputs: { app: 'test.wasm.views' } } };
      window.instances = [];
      await import('/packages/views.js');
      const host = { incarnation: 1, packageViewOwners: { 'swatch-box': '@fixture/swatch' },
        grantSet: { entries: [[1, 'fs.write app:/tmp', ['fs-write', 'app:', 'tmp'], null]], error: null, keepsNoAnswers: false, seal: '130293dc1db0c4d5', version: 1 } };
      const topics = [], options = { log() {}, dispatch() {} };
      let clock = 10;
      const session = { host, generation: 1, agentMode: true, now: () => clock, foreground: () => true, changed: key => topics.push(key) };
      const table = exact.wasmViewHost(session, options);
      const place = async id => {
        const el = document.createElement('swatch-box');
        el.dataset.view = id;
        el.dataset.nativeviewprops = '{"tint":"red","caption":"red"}';
        el.exactNative = { id, name: el.localName };
        document.getElementById('exact-root').append(el);
        host.packageViews.report();
        await table.attach(el);
        return el;
      };
      const [one, two] = await Promise.all([place(1), place(2)]);
      host.packageViews.report();
      const [{ impl, context }] = window.instances;
      await context.files.write('app:/tmp/allowed', new Uint8Array([3]));
      const denied = await context.files.write('app:/data/refused', new Uint8Array()).then(() => null, e => e.message);
      const reports = impl.swatchReports();
      const callbacks = [];
      context.after(7, () => callbacks.push(clock));
      context.after(3, () => callbacks.push('cancelled')).stop();
      clock = host.packageViews.nextDue;
      host.packageViews.report();
      host.grantSet = null;
      await context.files.write('app:/tmp/swatch/frame', new Uint8Array([4]));
      const directoryDenied = await context.files.list('app:/data').then(() => null, e => e.code);
      const siblingDenied = await context.files.write('app:/tmp/other/frame', new Uint8Array()).then(() => null, e => e.message);
      table.destroy(one);
      const destroyed = !one.firstChild;
      one.remove();
      host.packageViews.report();
      await Promise.resolve();
      const hidden = impl.swatchReports().text;
      table.destroy(two);
      host.incarnation++;
      host.packageViews.dispose();
      host.packageViews = null;
      exact.wasmViewHost(session, options);
      const stayedRetired = !host.packageViews;
      await Promise.resolve();
      const finalReports = impl.swatchReports().text;
      two.remove();
      const next = exact.wasmViewHost({ ...session, generation: 2 }, options);
      const el = document.createElement('swatch-box');
      el.exactNative = { id: 1, name: el.localName };
      await next.attach(el);
      return { count: window.instances.length, writable: Reflect.set(context, "development", !context.development), scoped: [window.instances[0].manifest, window.instances[0].resolved].map(m => Object.keys(m.development.packages)), reports, grants: context.grants, denied, destroyed, hidden, topics, callbacks, stayedRetired, siblingDenied, directoryDenied, finalReports };
    });
    expect(result.stayedRetired).toBe(true);
    expect(result.callbacks).toEqual([17]);
    expect(result.finalReports).toContain('displayed 2 false app');
    expect(result.directoryDenied).toBe('grant-missing');
    for (const id of [1, 2]) {
      expect(result.reports.text.indexOf(`made ${id}`)).toBeLessThan(result.reports.text.indexOf(`displayed ${id}`));
      expect(result.reports.text.indexOf(`made ${id}`)).toBeLessThan(result.reports.text.indexOf(`bound ${id}`));
    }
    expect(result.siblingDenied).toContain('denied: fs.write app:/tmp/other/frame');
    expect(result.count).toBe(2);
    expect(result.reports.config).toBe(`label=${production ? 'tiles' : canary} apiKey=4 chars ending base development=${!production}`);
    expect(result.writable).toBe(false);
    expect(result.scoped).toEqual(production ? [[], []] : [['@fixture/swatch'], ['@fixture/swatch']]);
    for (const id of [1, 2]) for (const report of [`made ${id}`, `displayed ${id} true app`, `bound ${id} tint app`]) expect(result.reports.text).toContain(report);
    expect(result.grants).toBe('fs.write app:/tmp');
    expect(result.denied).toContain('denied: fs.write app:/data/refused');
    expect(result.destroyed).toBe(true);
    expect(result.hidden).toContain('displayed 1 false app');
    expect(result.topics).toContain('swatch.swatchReports');
  } finally {
    if (previousKey === undefined) delete process.env.EXACT_WASM_SWATCH_KEY; else process.env.EXACT_WASM_SWATCH_KEY = previousKey;
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
});
