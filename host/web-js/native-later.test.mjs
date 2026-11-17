// `native.later` on the JS target (native.js over host/web's native-glue.js, staged beside it as a build
// stages them): a request reaches the page module's `later(request)` whatever its size.
import { afterAll, expect, test } from 'bun:test';
import { cpSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

const dir = mkdtempSync(resolve(tmpdir(), 'exact-native-later-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
for (const [from, to] of [['./native.js', 'native.js'], ['../web/native-glue.js', 'native-glue.js']]) cpSync(new URL(from, import.meta.url), resolve(dir, to));
globalThis.exact = { nativeArtifact: Promise.resolve({ later: request => ({ length: request.data.length, tail: request.data.slice(-4) }) }) };
const { pageModule } = await import(resolve(dir, 'native.js'));

test('a request of several megabytes, and text beyond ASCII, reach later(request) whole', async () => {
  const page = await pageModule({ changed() {}, agent: false, now: () => 0 });
  const data = 'x'.repeat(4 << 20) + 'é漢🎉';
  expect(await page.later({ op: 'echo', data })).toEqual({ length: data.length, tail: 'é漢🎉' });
});
