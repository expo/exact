import { expect, test } from 'bun:test';
import { rolldown } from 'rolldown';
import { asyncContext, lowered } from '../../js/bake/src/typescript.mjs';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { webCompiler } from './module.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

const root = resolve(import.meta.dir, '../..');
async function fixture(source = "export const appId='test.canvas'; export const answer=()=>captureCanvas('ink','app:/tmp/ink.png');") {
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-canvas-grants-'));
  let grants;
  const inputSource = resolve(scratch, 'source.js');
  writeFileSync(inputSource, source);
  const bundle = await rolldown({ input: inputSource, transform: lowered, plugins: [asyncContext()] });
  try { source = (await bundle.generate({ format: 'esm' })).output[0].code; }
  finally { await bundle.close(); }
  try {
    const input = resolve(scratch, 'grants.txt');
    writeFileSync(input, 'fs.write app:/tmp\nfs.read app:/tmp');
    const { cmd, pre } = webCompiler();
    if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const run = spawnSync(cmd, [...pre, 'normalize-grants', input], { encoding: 'utf8' });
    if (run.status !== 0) throw new Error(run.stderr);
    grants = JSON.parse(run.stdout);
  } finally { rmSync(scratch, { recursive: true }); }
  const glue = readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
    .replace('__APP_TS__', '/source.js').replace('__FETCH_MODULE__', './ts-fetch.js')
    .replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '');
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    const generated = {
      '/source.js': source,
      '/host/web-js/draw.js': `export const drawer = {
        draw(q) {
          globalThis.canvasDrawn?.(q);
          const bytes = new Uint8Array(88), v = new DataView(bytes.buffer);
          v.setUint32(0, 0x44324345, true); v.setUint32(4, 1, true);
          v.setUint32(8, 10, true); v.setUint32(12, 4, true);
          [Math.floor(q.frame[0]) % 251, 0, 0, 1].forEach((n, i) => v.setFloat64(16 + i * 8, n, true));
          v.setUint32(48, 53, true); v.setUint32(52, 4, true);
          [0, 0, 20, 20].forEach((n, i) => v.setFloat64(56 + i * 8, n, true));
          return { lists: [bytes], frame: true };
        }, retire() {}
      };`,
      '/host/web-js/ts-data.js': glue,
      '/host/web-js/names.js': 'export const sourceTypes = {};',
      '/host/web-js/admission-data.js': `import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(${JSON.stringify(grants)});`,
    }[path];
    if (generated) return new Response(generated, { headers: { 'content-type': 'text/javascript' } });
    const local = resolve(root, '.' + path), file = existsSync(local) ? local : resolve(root, 'host/web', path.split('/').at(-1));
    return existsSync(file) ? new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('', { status: 404 });
  } });
  const browser = await suiteBrowser(), page = await browser.newPage();
  await page.goto(server.url.href);
  return { page, grants, close: async () => { await browser.close(); server.stop(true); } };
}

test('a capture in a drawing turn includes its new arguments and size', async () => {
  const { page, grants, close } = await fixture();
  try {
    const observed = await page.evaluate(async grants => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      install(rt.data);
      const canvas = document.createElement('canvas');
      let step, size, mutation, captured;
      const snapshots = [];
      // The encoder takes its copy at invocation, before its asynchronous completion.
      globalThis.exact.canvas2dCapture = () => ({ width: canvas.width, height: canvas.height, scale: 1, png: () => {
        snapshots.push([canvas.width, ...canvas.getContext('2d').getImageData(0, 0, 1, 1).data]);
        return Promise.reject(new Error('snapshot recorded'));
      } });
      rt.mount(root => {
        root.append(canvas);
        step = rt.sig(0); size = rt.sig(10); captured = rt.sig(null);
        mutation = rt.mut('capture', captured, []);
        rt.effect(() => {
          canvas.width = size(); canvas.height = 10;
          const ctx = canvas.getContext('2d');
          ctx.fillStyle = step() ? '#0000ff' : '#ff0000'; ctx.fillRect(0, 0, canvas.width, 10);
        });
      });
      for (const width of [10, 20]) {
        rt.commit(() => { rt.W(step, 1); rt.W(size, width); rt.M(mutation, 'capture', []); });
        // The source's completion, including a deferred canvas read, gets its own turns.
        while (rt.inflight.n) await new Promise(r => setTimeout(r, 0));
        rt.commit(() => rt.W(step, 0));
      }
      const { createRequestExecutor, createGrantSet } = await import('/host/web-js/admission.js');
      globalThis.exact.canvas2dCapture = () => ({ width: canvas.width, height: canvas.height, scale: 1,
        png: () => new Promise(resolve => canvas.toBlob(resolve, 'image/png')) });
      const execute = createRequestExecutor('test.canvas', createGrantSet(grants));
      const capture = (scope, path = 'app:/tmp/rust.png') => execute({ canvas: { surface: 'ink', path }, scope });
      const allowed = await capture(null), refused = await capture(''), slashes = await capture(null, 'app://tmp/rust.png');
      const decode = reply => [reply.status, JSON.parse(new TextDecoder().decode(reply.body))];
      return { snapshots, allowed: decode(allowed), refused: decode(refused), slashes: decode(slashes) };
    }, grants);
    expect(observed.snapshots).toEqual([[10, 0, 0, 255, 255], [20, 0, 0, 255, 255]]);
    expect(observed.allowed).toEqual([200, { path: 'app:/tmp/rust.png', width: 20, height: 10, scale: 1 }]);
    expect(observed.slashes).toEqual([200, { path: 'app://tmp/rust.png', width: 20, height: 10, scale: 1 }]);
    expect(observed.refused[0]).toBe(400);
    expect(observed.refused[1].name).toBe('NotAllowedError');
  } finally { await close(); }
});

test('an unrelated frame task does not tick an animated canvas twice', async () => {
  const { page, close } = await fixture();
  try {
    const result = await page.evaluate(async () => {
      const raf = window.requestAnimationFrame.bind(window), counts = new Map();
      let frame;
      window.requestAnimationFrame = callback => raf(time => { frame = time; callback(time); });
      globalThis.canvasDrawn = q => {
        if (q.frame[2] & 8) counts.set(frame, (counts.get(frame) ?? 0) + 1);
      };
      const rt = await import('/host/web-js/rt.js');
      rt.mount(root => {
        const tick = rt.sig(0);
        rt.frames(() => rt.commit(() => rt.W(tick, tick() + 1)), 'unrelated');
        const el = document.createElement('div'), bitmap = document.createElement('canvas');
        el.style.cssText = 'width:20px;height:20px'; bitmap.dataset.surface = '';
        el.append(bitmap); root.append(el); rt.c2(el, 'ink', () => [], [], []);
      });
      while (counts.size < 5) await new Promise(raf);
      return [...counts.values()].slice(0, 4);
    });
    expect(result).toEqual([1, 1, 1, 1]);
  } finally { await close(); }
});

test('a mutation mounts and captures the first canvas in the same action', async () => {
  const { page, grants, close } = await fixture();
  try {
    const result = await page.evaluate(async grants => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      const { createGrantSet } = await import('/host/web-js/admission.js');
      const { createFileSystem } = await import('/host/web/storage-fs.js');
      install(rt.data);
      let shown, slot, mutation;
      rt.mount(root => {
        shown = rt.sig(false); slot = rt.sig(null); mutation = rt.mut('capture', slot, []);
        rt.when(root, shown, parent => {
          const el = document.createElement('div'), bitmap = document.createElement('canvas');
          el.style.cssText = 'width:20px;height:20px'; bitmap.dataset.surface = '';
          el.append(bitmap); parent.append(el); rt.c2(el, 'ink', () => [], [], []);
        });
      });
      rt.commit(() => { rt.W(shown, true); rt.M(mutation, 'capture', []); });
      while (rt.inflight.n) await new Promise(requestAnimationFrame);
      const fs = createFileSystem('test.canvas', createGrantSet(grants));
      try {
        const png = await createImageBitmap(new Blob([await fs.readFile('app:/tmp/ink.png')]));
        const size = [png.width, png.height]; png.close();
        return { size };
      } catch (e) { return { error: String(e), value: slot() }; }
    }, grants);
    expect(result).toEqual({ size: [20, 20] });
  } finally { await close(); }
});

// Shared with the runner: AppDirectories paths naming a file, bounded to 4096 bytes.
test('capture paths agree with the runner, including repeated slashes', async () => {
  const { captureCanvas } = await import('./canvas-capture.js');
  const vectors = JSON.parse(readFileSync(resolve(root, 'runner/tests/fixtures/canvas-capture-paths.json'), 'utf8'));
  for (const [bytes, valid] of [[4096, true], [4097, false]]) vectors.push(['app:/tmp/' + 'a'.repeat(bytes - 9), valid]);
  const ready = Promise.resolve({ width: 1, height: 1, scale: 1, png: async () => new Blob(['png']) });
  for (const [path, valid] of vectors) {
    let written = false, error;
    try { await captureCanvas('ink', path, async () => { written = true; }, ready); } catch (e) { error = e; }
    expect(written, JSON.stringify(path)).toBe(valid);
    if (!valid) expect(error?.name).toBe('NotAllowedError');
  }
});

for (const language of ['TypeScript', 'Rust']) {
  for (const scenario of ['cancel', 'publish', 'delay', 'replace', 'await-cancel', 'drawing']) {
    if (language === 'TypeScript' && ['replace', 'drawing'].includes(scenario) || language === 'Rust' && scenario === 'await-cancel') continue;
    test(`${language} capture ${scenario} preserves its ticket and bitmap`, async () => {
      const { page, grants, close } = await fixture(scenario === 'await-cancel' ? "export const appId='test.canvas'; export const answer=async()=>{ await Promise.resolve(); return captureCanvas('ink','app:/tmp/ink.png'); };" : undefined);
      let releaseImport;
      if (scenario === 'delay') {
        const gate = new Promise(r => { releaseImport = r; });
        await page.route('**/canvas-capture.js', async route => { await gate; await route.continue(); });
      }
      try {
        await page.evaluate(async ({ grants, language, scenario }) => {
          const rt = await import('/host/web-js/rt.js');
          const { install } = await import('/host/web-js/ts-data.js');
          const { createRequestExecutor, createGrantSet } = await import('/host/web-js/admission.js');
          const { createFileSystem, lockPaths } = await import('/host/web/storage-fs.js');
          const fs = createFileSystem('test.canvas', createGrantSet(grants));
          await fs.atomicWriteFile('app:/tmp/ink.png', new TextEncoder().encode('keep'));
          const releaseFile = scenario === 'publish' ? await lockPaths('test.canvas', ['app:/tmp/ink.png']) : null;
          install(rt.data);
          const execute = createRequestExecutor('test.canvas', createGrantSet(grants));
          if (language === 'Rust') {
            rt.data.answer = (_name, args) => ({ req: { canvas: { surface: args[0] && scenario !== 'drawing' ? 'other' : 'ink', path: args[0] && scenario !== 'drawing' ? 'app:/tmp/other.png' : 'app:/tmp/ink.png' }, scope: null } });
            rt.data.fetch = execute;
            rt.data.parse = (_name, _args, reply) => ({ v: JSON.parse(new TextDecoder().decode(reply.body)) });
          }
          const canvas = document.createElement('canvas'); canvas.width = canvas.height = 10;
          const paint = color => { const ctx = canvas.getContext('2d'); ctx.fillStyle = color; ctx.fillRect(0, 0, canvas.width, canvas.height); };
          paint('red');
          const pending = [];
          globalThis.exact.canvas2dCapture = () => {
            const width = canvas.width, height = canvas.height;
            return { width, height, scale: 1, png: () => new Promise(resolve => {
              canvas.toBlob(blob => { if (scenario === 'cancel' || scenario === 'await-cancel' || scenario === 'replace' || scenario === 'drawing') pending.push(() => resolve(blob)); else resolve(blob); });
            }) };
          };
          let slot, mutation, arg, resource;
          rt.mount(root => {
            root.append(canvas); slot = rt.sig(null); mutation = rt.mut('capture', slot, []);
            if (scenario === 'replace' || scenario === 'drawing') { arg = rt.sig(0); resource = rt.res('capture', 'capture', () => [arg()], null); rt.effect(() => { resource(); if (arg()) { canvas.width = 20; paint('blue'); } }); }
          });
          if (scenario !== 'replace' && scenario !== 'drawing') rt.commit(() => rt.M(mutation, 'capture', []));
          globalThis.testCapture = { rt, fs, pending, slot, mutation, arg, resource, canvas, paint, releaseFile };
        }, { grants, language, scenario });
        if (scenario === 'publish') await page.waitForFunction(async () => (await navigator.locks.query()).pending.length > 0);
        else if (scenario !== 'delay') await page.waitForFunction(() => testCapture.pending.length === 1);
        await page.evaluate(scenario => {
          const t = testCapture;
          if (scenario === 'publish') { t.rt.commit(() => t.rt.W(t.slot, null)); t.releaseFile(); }
          else if (scenario === 'cancel' || scenario === 'await-cancel') { t.rt.commit(() => t.rt.W(t.slot, null)); t.pending.shift()(); }
          else if (scenario === 'replace' || scenario === 'drawing') t.rt.commit(() => t.rt.W(t.arg, 1));
          else { t.canvas.width = 20; t.paint('blue'); }
        }, scenario);
        releaseImport?.();
        if (scenario === 'replace' || scenario === 'drawing') {
          // A different request must start before the old encoder finishes.
          await page.waitForFunction(() => testCapture.pending.length === 2, null, { timeout: 3000 });
          await page.evaluate(() => testCapture.pending.splice(0).forEach(f => f()));
        }
        await page.waitForFunction(() => testCapture.rt.inflight.n === 0);
        const result = await page.evaluate(async scenario => {
          const t = testCapture, bytes = await t.fs.readFile('app:/tmp/ink.png');
          if (scenario === 'cancel' || scenario === 'await-cancel' || scenario === 'publish' || scenario === 'replace') return { kept: new TextDecoder().decode(bytes), other: scenario === 'replace' ? (await t.fs.readFile('app:/tmp/other.png')).byteLength : 0 };
          if (new TextDecoder().decode(bytes) === 'keep') return { refused: true };
          const image = await createImageBitmap(new Blob([bytes]));
          const c = document.createElement('canvas'); c.width = image.width; c.height = image.height; c.getContext('2d').drawImage(image, 0, 0);
          return { width: image.width, pixel: [...c.getContext('2d').getImageData(0, 0, 1, 1).data] };
        }, scenario);
        if (scenario === 'delay') { expect(result.width).toBe(10); expect(result.pixel).toEqual([255, 0, 0, 255]); }
        else if (scenario === 'drawing') { expect(result.width).toBe(20); expect(result.pixel).toEqual([0, 0, 255, 255]); }
        else { expect(result.kept).toBe('keep'); if (scenario === 'replace') expect(result.other).toBeGreaterThan(4); }
      } finally { releaseImport?.(); await close(); }
    });
  }
}

test('an overlay cannot start a canvas capture', async () => {
  const { page, grants, close } = await fixture();
  try {
    const result = await page.evaluate(async grants => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      const { overlaying, createGrantSet } = await import('/host/web-js/admission.js');
      const { createFileSystem } = await import('/host/web/storage-fs.js');
      const fs = createFileSystem('test.canvas', createGrantSet(grants));
      await fs.atomicWriteFile('app:/tmp/ink.png', new TextEncoder().encode('keep'));
      install(rt.data);
      let captures = 0;
      globalThis.exact.canvas2dCapture = () => { captures++; return { width: 1, height: 1, scale: 1, png: async () => new Blob(['png']) }; };
      overlaying.on = true;
      const promise = captureCanvas('ink', 'app:/tmp/ink.png').then(() => null, e => e.message);
      overlaying.on = false;
      return { error: await promise, captures, kept: new TextDecoder().decode(await fs.readFile('app:/tmp/ink.png')) };
    }, grants);
    expect(result.kept).toBe('keep');
    expect(result.captures).toBe(0);
    expect(result.error).toContain('overlay');
  } finally { await close(); }
});

test('a plain then callback cannot publish after its answer is cancelled behind a file lock', async () => {
  const { page, grants, close } = await fixture(`export const appId = 'test.canvas';
export const answer = () => Promise.resolve().then(() => captureCanvas('ink', 'app:/tmp/ink.png'))
  .catch(e => { globalThis.captureError = e.message; }).finally(() => { globalThis.captureDone = true; });`);
  try {
    await page.evaluate(async grants => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      const { createGrantSet } = await import('/host/web-js/admission.js');
      const { createFileSystem, lockPaths } = await import('/host/web/storage-fs.js');
      install(rt.data);
      const fs = createFileSystem('test.canvas', createGrantSet(grants));
      await fs.atomicWriteFile('app:/tmp/ink.png', new TextEncoder().encode('keep'));
      const release = await lockPaths('test.canvas', ['app:/tmp/ink.png']);
      globalThis.exact.canvas2dCapture = () => ({ width: 1, height: 1, scale: 1, png: async () => new Blob(['png']) });
      let slot, mutation;
      rt.mount(() => { slot = rt.sig(null); mutation = rt.mut('capture', slot, []); });
      rt.commit(() => rt.M(mutation, 'capture', []));
      globalThis.testThen = { rt, fs, slot, release };
    }, grants);
    await page.waitForFunction(async () => globalThis.captureDone || (await navigator.locks.query()).pending.length > 0);
    await page.evaluate(() => { testThen.rt.commit(() => testThen.rt.W(testThen.slot, null)); testThen.release(); });
    await page.waitForFunction(() => globalThis.captureDone);
    const result = await page.evaluate(async () => ({
      kept: new TextDecoder().decode(await testThen.fs.readFile('app:/tmp/ink.png')), error: globalThis.captureError,
    }));
    expect(result.kept).toBe('keep');
    expect(result.error).toContain('live answer');
  } finally { await close(); }
});

for (const count of [0, 1, 2]) test(`a capture pins ${count} canvases before drawer readiness`, async () => {
  const { page, close } = await fixture();
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/draw.js', async route => { await gate; await route.continue(); });
  try {
    await page.evaluate(async count => {
      const rt = await import('/host/web-js/rt.js');
      rt.clock.agent = true;
      let first, second;
      const mount = (parent, name = 'ink') => {
        const el = document.createElement('div'), bitmap = document.createElement('canvas');
        el.style.cssText = 'width:20px;height:20px'; bitmap.dataset.surface = '';
        el.append(bitmap); parent.append(el); rt.c2(el, name, () => [], [], []);
      };
      rt.mount(root => {
        first = rt.sig(count > 0); second = rt.sig(count > 1);
        mount(root, 'other');
        rt.when(root, first, parent => mount(parent)); rt.when(root, second, parent => mount(parent));
      });
      rt.commit(() => { globalThis.pinnedCapture = rt.canvasCommitted('ink'); });
      rt.commit(() => { rt.W(first, true); rt.W(second, count === 1); });
    }, count);
    release();
    const result = await page.evaluate(async () => {
      const capture = await pinnedCapture;
      if (capture.name) return capture.name;
      const image = await createImageBitmap(await capture.png());
      const size = [image.width, image.height]; image.close(); return size;
    });
    expect(result).toEqual(count === 0 ? 'NotFoundError' : count === 2 ? 'InvalidStateError' : [20, 20]);
  } finally { release(); await close(); }
});

for (const later of [false, true]) test(`a stream mapper cannot capture with a locked destination${later ? ' after await' : ''}`, async () => {
  const { page, grants, close } = await fixture(`export const appId = 'test.canvas';
export const answer = () => '';
export async function map(event) {
  if (event.type !== 'message') return '';
  ${later ? 'await Promise.resolve();' : ''}
  try { await captureCanvas('ink', 'app:/tmp/ink.png'); }
  catch (e) { globalThis.captureError = e.message; }
  finally { globalThis.captureDone = true; }
  return '';
}`);
  await page.route('**/host/web-js/http-body.js', route => route.fulfill({ contentType: 'text/javascript', body: `
export function streamed(req, grants, message, controller) {
  message({ event: '', id: '', data: 'capture', coalesced: 0 });
  return new Promise(resolve => controller.signal.addEventListener('abort', () => resolve({ kind: 4, body: new Uint8Array() }), { once: true }));
}` }));
  try {
    await page.evaluate(async grants => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      const { createGrantSet } = await import('/host/web-js/admission.js');
      const { createFileSystem, lockPaths } = await import('/host/web/storage-fs.js');
      const { open } = await import('/host/web-js/ts-stream.js');
      const { map } = await import('/source.js');
      install(rt.data);
      rt.mount(() => {});
      const fs = createFileSystem('test.canvas', createGrantSet(grants));
      await fs.atomicWriteFile('app:/tmp/ink.png', new TextEncoder().encode('keep'));
      const release = await lockPaths('test.canvas', ['app:/tmp/ink.png']);
      globalThis.captures = 0;
      globalThis.exact.canvas2dCapture = () => { captures++; return { width: 1, height: 1, scale: 1, png: async () => new Blob(['png']) }; };
      const controller = new AbortController();
      const ended = open({ input: 'https://example.test/events', init: { exactStream: map } }, v => v, null, () => {}, controller);
      globalThis.testMapper = { fs, controller, release, ended };
    }, grants);
    await page.waitForFunction(async () => globalThis.captureDone || (await navigator.locks.query()).pending.length > 0);
    await page.evaluate(() => { testMapper.controller.abort(); testMapper.release(); });
    await page.waitForFunction(() => globalThis.captureDone);
    const result = await page.evaluate(async () => {
      await testMapper.ended;
      return { kept: new TextDecoder().decode(await testMapper.fs.readFile('app:/tmp/ink.png')), captures, error: globalThis.captureError };
    });
    expect(result.kept).toBe('keep');
    expect(result.captures).toBe(0);
    expect(result.error).toContain("stream's mapper");
  } finally { await close(); }
});

// The later-commits and snapshotted rows of runner's captures_wait_for_a_completed_drawing.
test('later commits can supply a capture and a snapshot keeps its pixels', async () => {
  const { page, close } = await fixture();
  try {
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = 2;
      const draw = color => { const c = canvas.getContext('2d'); c.fillStyle = color; c.fillRect(0, 0, 2, 2); };
      draw('red');
      rt.mount(root => root.append(canvas));
      globalThis.exact ??= {};
      globalThis.exact.canvas2dCapture = () => ({ width: 2, height: 2, scale: 1, png: () => new Promise(resolve => canvas.toBlob(resolve)) });
      let ready;
      rt.clock.timers.push({ once: true, due: 1, action: () => rt.commit(() => { ready = rt.canvasCommitted('ink'); }) });
      rt.clock.timers.push({ once: true, due: 1, action: () => rt.commit(() => draw('blue')) });
      rt.advance(1);
      const capture = await ready;
      rt.commit(() => draw('green'));
      const image = await createImageBitmap(await capture.png());
      const copy = document.createElement('canvas'); copy.width = copy.height = 2;
      const c = copy.getContext('2d'); c.drawImage(image, 0, 0); image.close();
      return [...c.getImageData(0, 0, 1, 1).data];
    });
    expect(result).toEqual([0, 0, 255, 255]);
  } finally { await close(); }
});

// Real animation callbacks, engine and browser replayer; only the source's drawing is a fixture.
test('a live frame task captures the time-dependent drawing after its commit', async () => {
  const { page, close } = await fixture();
  try {
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      rt.mount(root => {
        const el = document.createElement('div'), bitmap = document.createElement('canvas');
        el.style.cssText = 'width:20px;height:20px'; bitmap.dataset.surface = '';
        el.append(bitmap); root.append(el); rt.c2(el, 'ink', () => [], [], []);
      });
      while (!exact.canvas2dState?.()[0]?.draws) await new Promise(requestAnimationFrame);
      const rows = [];
      await new Promise((resolve, reject) => {
        let started = 0;
        rt.frames(() => {
          if (started++ >= 3) return;
          let ready, time;
          rt.commit(() => { time = rt.x_performanceNow(); ready = rt.canvasCommitted('ink'); });
          ready.then(async capture => {
            const image = await createImageBitmap(await capture.png());
            const copy = document.createElement('canvas'); copy.width = copy.height = 20;
            const ctx = copy.getContext('2d'); ctx.drawImage(image, 0, 0); image.close();
            rows.push([Math.floor(time) % 251, ctx.getImageData(1, 1, 1, 1).data[0]]);
            if (rows.length === 3) resolve();
          }).catch(reject);
        }, 'capture');
      });
      return rows;
    });
    expect(result).toHaveLength(3);
    for (const [expected, pixel] of result) expect(pixel).toBe(expected);
  } finally { await close(); }
});

for (const row of ['later commits', 'several frames', 'unmount', 'replace']) test(`the real canvas engine: ${row} before snapshot`, async () => {
  const { page, close } = await fixture();
  try {
    const result = await page.evaluate(async row => {
      const rt = await import('/host/web-js/rt.js');
      rt.clock.agent = true;
      let shown;
      rt.mount(root => {
        shown = rt.sig(true);
        rt.when(root, shown, parent => {
          const el = document.createElement('div'), bitmap = document.createElement('canvas');
          el.style.cssText = 'width:20px;height:20px'; bitmap.dataset.surface = '';
          el.append(bitmap); parent.append(el); rt.c2(el, 'ink', () => [], [], []);
        });
      });
      rt.clock.agent = true;
      while (!exact.canvas2dState?.()[0]?.draws) await new Promise(requestAnimationFrame);
      let ready;
      rt.commit(() => { ready = rt.canvasCommitted('ink'); });
      if (row === 'unmount' || row === 'replace') {
        rt.commit(() => rt.W(shown, false));
        if (row === 'replace') rt.commit(() => rt.W(shown, true));
      } else {
        for (const time of [16, 32, 48]) {
          rt.advance(time);
          rt.commit(() => {});
          if (row === 'several frames') exact.canvas2dFrame();
        }
      }
      const capture = await ready;
      if (capture.name) return capture.name;
      const image = await createImageBitmap(await capture.png());
      const copy = document.createElement('canvas'); copy.width = copy.height = 20;
      const ctx = copy.getContext('2d'); ctx.drawImage(image, 0, 0); image.close();
      return ctx.getImageData(1, 1, 1, 1).data[0];
    }, row);
    expect(result).toBe(row === 'unmount' || row === 'replace' ? 'EncodingError' : 48);
  } finally { await close(); }
});
