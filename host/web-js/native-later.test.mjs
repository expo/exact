// `native.later` on the JS target (native.js over host/web's native-glue.js, staged beside it as a build
// stages them): a request reaches the page module's `later(request)` whatever its size.
import { afterAll, expect, test } from 'bun:test';
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { localImports } from './glue.mjs';

const dir = mkdtempSync(resolve(tmpdir(), 'exact-native-later-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
for (const [from, to] of [['./native.js', 'native.js'], ['../web/native-glue.js', 'native-glue.js'], ['../web/load-once.js', 'load-once.js']]) writeFileSync(resolve(dir, to), localImports(readFileSync(new URL(from, import.meta.url), 'utf8')));
globalThis.exact = { nativeArtifact: Promise.resolve({ later: request => ({ length: request.data.length, tail: request.data.slice(-4) }) }) };
const { pageModule } = await import(resolve(dir, 'native.js'));

test('a request of several megabytes, and text beyond ASCII, reach later(request) whole', async () => {
  const page = await pageModule({ changed() {}, agent: false, now: () => 0 });
  const data = 'x'.repeat(4 << 20) + 'é漢🎉';
  expect(await page.later({ op: 'echo', data })).toEqual({ length: data.length, tail: 'é漢🎉' });
});

test('a refused page module can load on the next request', async () => {
  const before = globalThis.exact.pageNative;
  let attempts = 0;
  globalThis.exact.pageNative = () => ++attempts === 1 ? Promise.reject(new TypeError('network'))
    : Promise.resolve({ later: async () => JSON.stringify({ ok: true }) });
  try {
    const { pageModule } = await import(resolve(dir, 'native.js') + '?retry');
    const options = { changed() {}, agent: false, now: () => 0 };
    await expect(pageModule(options)).rejects.toThrow('network');
    const page = await pageModule(options);
    expect(await page.later({})).toEqual({ ok: true });
    await pageModule(options);
    expect(attempts).toBe(2);
  } finally { globalThis.exact.pageNative = before; }
});
