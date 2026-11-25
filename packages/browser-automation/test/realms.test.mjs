import { expect, test } from 'bun:test';
import { $ } from 'bun';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

async function inFrame(check) {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    const source = await Bun.file(new URL('../web/index.js', import.meta.url)).text();
    await page.route('http://wallet.test/**', route => {
      const path = new URL(route.request().url()).pathname;
      if (path === '/binding.js') return route.fulfill({ contentType: 'text/javascript', body: source });
      const body = path === '/' ? '<div id="exact-root"><iframe id="tab" src="/wallet"></iframe></div>' : '<title>Wallet</title>';
      return route.fulfill({ contentType: 'text/html', body });
    });
    await page.goto('http://wallet.test/');
    return await page.evaluate(check);
  } finally {
    await browser.close();
  }
}

test('ordinary frame calls work with a traced evaluator and a non-extensible document', async () => {
  expect(await inFrame(async () => {
    const api = (await import('/binding.js')).implementation();
    const win = document.querySelector('iframe').contentWindow;
    const evaluate = win.eval;
    win.eval = source => evaluate(source);
    const traced = await api.evaluate({ id: 'tab', expression: '21 * 2' });
    win.eval = evaluate;
    Object.preventExtensions(win.document);
    const sealed = await api.callFunction({ id: 'tab', functionDeclaration: 'x => x * 2', arguments: [21] });
    return [traced, sealed];
  })).toEqual([{ result: 42 }, { result: 42 }]);
}, 60_000);

test('a context reaches its frame and a realm target is unsupported on the web', async () => {
  expect(await inFrame(async () => {
    const api = (await import('/binding.js')).implementation();
    const context = await api.evaluate({ target: { context: 'tab' }, expression: 'document.title' });
    const refused = await api.callFunction({ target: { realm: 'old' }, functionDeclaration: '() => 42' });
    return [context, refused.error, refused.message.includes('realm')];
  })).toEqual([{ result: 'Wallet' }, 'NotSupportedError', true]);
}, 60_000);

test('a channel is refused by name before page code runs', async () => {
  expect(await inFrame(async () => {
    const api = (await import('/binding.js')).implementation();
    const args = { id: 'tab', functionDeclaration: '() => window.invoked = true',
      arguments: [{ type: 'channel', value: { channel: 'playback' } }] };
    const refusals = await Promise.all([api.callFunction(args), api.callFunction({ ...args, sandbox: 'video' }), api.addPreloadScript(args)]);
    const ordinary = await api.callFunction({id: 'tab', arguments: [{type: 'channel', name: 'news'}], functionDeclaration: 'value => value.name'});
    const payload = await api.callFunction({id: 'tab', arguments: [{type: 'channel', value: {channel: 'news', payload: 1}}],
      functionDeclaration: 'x => x.value.payload'});
    return [ordinary.result, payload.result, refusals.every(r => r.error === 'NotSupportedError' && r.message.includes('script.message')),
      typeof document.querySelector('iframe').contentWindow.invoked];
  })).toEqual(['news', 1, true, 'undefined']);
}, 60_000);

test.skipIf(process.platform !== 'darwin')('a post during the overflow report keeps the last 32 posts', async () => {
  const root = resolve(import.meta.dir, '../../..');
  const out = await mkdtemp(join(tmpdir(), 'exact-player-queue-'));
  try {
    await $`xcrun swiftc -parse-as-library -emit-library -static -module-name ExactPackage -emit-module -emit-module-path ${out}/ExactPackage.swiftmodule ${root}/host/apple/modules/ExactPackage.swift -o ${out}/libExactPackage.a`.quiet();
    const module = await Bun.file(join(import.meta.dir, '../apple/BrowserAutomation.swift')).text();
    const test = await Bun.file(join(import.meta.dir, 'Queue.swift')).text();
    await Bun.write(join(out, 'Queue.swift'), module + '\n' + test);
    await $`xcrun swiftc -parse-as-library -I ${out} -L ${out} -lExactPackage ${root}/host/apple/webarm/ScriptMessageQueue.swift ${out}/Queue.swift -o ${out}/queue`.quiet();
    await $`${out}/queue`.quiet();
  } finally {
    await rm(out, { recursive: true, force: true });
  }
}, 120_000);
