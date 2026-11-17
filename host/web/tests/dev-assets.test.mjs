// The dev page reads a generation's assets/ and deck/ files when the page asks for them (dev.js `later`).
import {test, expect} from 'bun:test';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {createHash} from 'node:crypto';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const encoder = new TextEncoder();

test('a generation reads its assets when asked, verified, and keeps them for the next', async () => {
  let stream, seq = 0;
  const files = new Map(), fetched = [], booted = [], errors = [];
  const sources = {'assets/exact-tables.json': encoder.encode('{}'), 'assets/used.tex': Uint8Array.of(1, 2), 'assets/unused.tex': Uint8Array.of(3)};
  const context = {TextEncoder, TextDecoder, URL, URLSearchParams, AbortController, AbortSignal, Response, setTimeout, clearTimeout, performance, crypto,
    console: {error: (...a) => errors.push(a.join(' '))}, location: {origin: 'http://localhost', href: 'http://localhost/', search: ''},
    EventSource: class { constructor() { stream = this; } },
    document: {body: {appendChild: el => el}, createElement: () => ({remove() {}}), querySelector: () => null},
    navigator: {sendBeacon() {}}, requestAnimationFrame: callback => callback(), addEventListener() {},
    matchMedia: () => ({matches: false, addEventListener() {}}), localStorage: {getItem: () => '1', setItem() {}},
    fetch: async url => { const path = new URL(url, 'http://localhost/').pathname; if (path.startsWith('/g/')) fetched.push(path); return new Response(files.get(path) ?? '{}'); },
    exact: {ready: Promise.resolve(), compat: {inputs: {app: 'fixture'}},
      reloadGeneration: async (plan, assets) => { booted.push(assets); return true; }},
  };
  runInNewContext(readFileSync(new URL('../dev.js', import.meta.url), 'utf8'), context);
  const generation = async (corrupt = false) => {
    seq += 1;
    const prefix = `http://localhost/g/${seq}/`, plan = Uint8Array.of(seq);
    const cards = Object.entries(sources).map(([name, bytes]) => ({name, url: prefix + name, bytes: bytes.length, sha256: hash(bytes)}));
    for (const [name, bytes] of Object.entries(sources)) files.set(new URL(prefix + name).pathname, corrupt && name === 'assets/unused.tex' ? Uint8Array.of(9) : bytes);
    const index = encoder.encode(JSON.stringify({assets: cards}));
    files.set(`/g/${seq}/assets.json`, index);
    files.set(`/g/${seq}/app.plan`, plan);
    const canonical = {assets: cards.map(({bytes, name, sha256}) => ({bytes, name, sha256})).sort((a, b) => a.name < b.name ? -1 : 1), plan: {bytes: 1, sha256: hash(plan)}};
    const dev = {epoch: 'a'.repeat(32), seq, generation: hash(JSON.stringify(canonical))};
    files.set(`/g/${seq}/exact.json`, encoder.encode(JSON.stringify({exact: 1, app: {id: 'fixture'}, dev,
      plan: {url: prefix + 'app.plan', bytes: 1, sha256: hash(plan)}, assets: {url: prefix + 'assets.json', bytes: index.length, sha256: hash(index), count: cards.length}})));
    fetched.length = 0;
    const before = booted.length;
    stream.onmessage({data: JSON.stringify({...dev, envelope: prefix + 'exact.json'})});
    // The page hashes what it fetched on no fixed turn: wait for its boot, at most 2 s.
    for (let i = 0; i < 400 && booted.length === before; i++) await new Promise(r => setTimeout(r, 5));
    return booted.at(-1);
  };

  const first = await generation();
  expect(errors).toEqual([]);
  expect(fetched.sort()).toEqual(['/g/1/app.plan', '/g/1/assets.json', '/g/1/assets/exact-tables.json', '/g/1/exact.json']);
  expect(first.get('assets/used.tex').bytes).toBe(null);
  expect([...await first.get('assets/used.tex').read()]).toEqual([1, 2]);
  await first.get('assets/used.tex').read();
  expect(fetched.filter(path => path.endsWith('.tex'))).toEqual(['/g/1/assets/used.tex']);

  // The next generation has the file unchanged: the bytes already read are its own.
  const second = await generation();
  expect([...second.get('assets/used.tex').bytes]).toEqual([1, 2]);
  expect(second.get('assets/unused.tex').bytes).toBe(null);
  expect(fetched.filter(path => path.endsWith('.tex'))).toEqual([]);

  // A file whose bytes do not match its card is refused when read.
  const third = await generation(true);
  await expect(third.get('assets/unused.tex').read()).rejects.toThrow('payload integrity failed: assets/unused.tex');
});
