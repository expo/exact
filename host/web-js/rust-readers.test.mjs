import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
// The web Rust adapter against a stand-in for its wasm module (LLP 1146 §2.3).
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { webCompiler } from './module.mjs';
import { createGrantSet, createRequestExecutor } from './admission.js';

const ROOT = resolve(import.meta.dir, '../..');

// `spec` as the web's grant set, by the grammar's own normalizer (`exact-web-js normalize-grants`), the compiler the
// lane built.
function normalized(spec) {
  const { cmd, pre } = webCompiler();
  if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-grants-')), input = resolve(scratch, 'grants.txt');
  writeFileSync(input, spec);
  const run = spawnSync(cmd, [...pre, 'normalize-grants', input], { cwd: ROOT, encoding: 'utf8' });
  rmSync(scratch, { recursive: true });
  if (run.status !== 0) throw new Error(run.stderr || 'the grant normalizer failed');
  return JSON.parse(run.stdout);
}

test('a render refuses a Rust storage request after validating its scope', async () => {
  const run = createRequestExecutor('test.rust', createGrantSet(normalized('fs.read app:/data')));
  const storage = JSON.stringify({ version: 1, op: 'fs.readFile', args: { path: 'app:/data/prefs' } });
  const hadRender = Object.hasOwn(globalThis, '__exactRender'), before = globalThis.__exactRender;
  globalThis.__exactRender = true;
  try {
    expect(await run({ storage, scope: 'fs.write app:/data' })).toEqual({ failed: 2, message: "source scope exceeds the app's admitted grants" });
    expect(await run({ storage })).toEqual({ refused: 'storage request' });
    expect(await run({ storage, scope: '' })).toEqual({ refused: 'storage request' });
  } finally {
    if (hadRender) globalThis.__exactRender = before; else delete globalThis.__exactRender;
  }
});

// The web Rust adapter carries a store read its call made to rt.js (LLP 1146 §2.3): on the answer when the call
// answers, and through `data.threwRead` when it fails, an answer's or a parse's, so the resource is a store reader
// either way, as natively.
test("a Rust source's store read reaches rt.js whether its call answers or fails", async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-rust-readers-'));
  const set = normalized('secret.keep k');
  for (const name of ['rust-data.js', 'admission.js']) {
    const source = readFileSync(resolve(ROOT, 'host/web-js', name), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'");
    writeFileSync(resolve(dir, name), source);
  }
  for (const name of ['http-body.js', 'grant-admission.js', 'faults.js', 'navigation.js', 'address-space.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
  writeFileSync(resolve(dir, 'admission-data.js'), `import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(${JSON.stringify(set)}),tsGrantSet=createGrantSet(${JSON.stringify(normalized(''))});\n`);
  const memory = new WebAssembly.Memory({ initial: 1 }), out = 32768;
  let output = new Uint8Array(), read = false, fails = false, unknown = false, socket = false;
  const response = fill => {
    const bytes = [], u8 = value => bytes.push(value), u32 = value => bytes.push(value & 255, value >>> 8 & 255, value >>> 16 & 255, value >>> 24 & 255);
    const string = value => { const encoded = new TextEncoder().encode(value); u32(encoded.length); bytes.push(...encoded); };
    u32(5); fill({ u8, u32, string }); output = Uint8Array.from(bytes); new Uint8Array(memory.buffer, out, output.length).set(output);
  };
  const exports = {
    memory, exact_logic_abi: () => 5, exact_logic_create: () => 1, exact_logic_alloc: () => 0, exact_logic_dealloc() {},
    exact_logic_output: () => out, exact_logic_output_len: () => output.length,
    exact_logic_call(_session, pointer) {
      const code = new Uint8Array(memory.buffer)[pointer + 4];
      if (code === 0) response(w => { w.u8(0); w.string('test.rust'); w.string('secret.keep k'); });
      else if (code === 1 || code === 2) response(w => { w.u8(0); w.u8(0); w.u8(3); });
      // A call's reply: whether it read the store, no writes, then a source it lacks (tag 2), a failure (tag 3) or a
      // request (tag 1).
      else if (code === 3 || code === 4) response(w => {
        w.u8(2); w.u8(read ? 1 : 0); w.u32(0); w.u32(0); w.u8(0); w.string('');
        if (socket) { w.u8(11); w.u32(1 << 20); w.u8(0); w.string(''); w.string('GET'); w.string('wss://api.example/book'); w.u32(0); w.u32(0); } else if (unknown) { w.u8(2); w.string('reader'); } else if (fails) { w.u8(3); w.string('nothing kept'); } else { w.u8(1); w.u8(0); w.string(''); w.string('GET'); w.string('https://api.example/me'); w.u32(0); w.u32(0); }
      });
      else throw new Error(`unexpected logic call ${code}`);
      return 0;
    },
  };
  const instantiate = WebAssembly.instantiate;
  WebAssembly.instantiate = async () => ({ instance: { exports } });
  try {
    const module = await import(`${pathToFileURL(resolve(dir, 'rust-data.js')).href}?readers=${Date.now()}`);
    const data = { q: [] }, store = { map: new Map(), set() {} }, failed = { failed: 3, message: 'refused' };
    await module.install(data, { reader: '' }, async () => new ArrayBuffer(0));
    [read, fails] = [true, false];
    expect(data.answer('reader', [], store).store).toBe(true);
    expect(data.parse('reader', [], failed, store).store).toBe(true);
    expect(data.threwRead()).toBe(false);
    [read, fails] = [true, true];
    expect(() => data.answer('reader', [], store)).toThrow('nothing kept');
    expect([data.threwRead(), data.threwRead()]).toEqual([true, false]);
    expect(() => data.parse('reader', [], failed, store)).toThrow('nothing kept');
    expect(data.threwRead()).toBe(true);
    // A Rust source's two-way socket (LLP 1016.003) is refused by name, never opened to listen only.
    [read, fails, socket] = [false, false, true];
    expect(() => data.answer('reader', [], store)).toThrow("a Rust source's two-way socket is not carried on the web yet");
    socket = false;
    [read, fails] = [false, true];
    expect(() => data.answer('reader', [], store)).toThrow('nothing kept');
    expect(data.threwRead()).toBe(false);
    // A mixed app: the Rust module reads and names a source it lacks, and the TypeScript one, asked in its place,
    // reads and throws. One failure: its read is told once, and nothing of it is left for the next.
    let tsRead = false;
    const mixed = { q: [], ts: () => { tsRead = true; throw new Error('the TypeScript source threw'); }, threwRead: () => [tsRead, tsRead = false][0] };
    await module.install(mixed, { reader: '' }, async () => new ArrayBuffer(0));
    [read, fails, unknown] = [true, false, true];
    expect(() => mixed.answer('reader', [], store)).toThrow('the TypeScript source threw');
    expect([mixed.threwRead(), mixed.threwRead()]).toEqual([true, false]);
    [read, fails, unknown] = [false, true, false];
    expect(() => mixed.answer('reader', [], store)).toThrow('nothing kept');
    expect(mixed.threwRead()).toBe(false);
  } finally {
    WebAssembly.instantiate = instantiate;
    rmSync(dir, { recursive: true, force: true });
  }
});
