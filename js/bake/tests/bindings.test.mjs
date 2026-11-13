import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseSync } from 'rolldown/utils';
import { check, configure, implementationFor, moduleInterfaces } from '../src/typescript.mjs';

test('generated json bindings narrow opaque values and retain known DOM dictionaries', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-json-bindings-'));
  const app = join(root, 'app'), stage = join(root, 'stage');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  try {
    mkdirSync(join(app, 'node_modules/@exact'), { recursive: true });
    mkdirSync(stage);
    for (const name of ['browser-automation', 'passkeys']) {
      symlinkSync(resolve(import.meta.dir, '../../../packages', name), join(app, 'node_modules/@exact', name), process.platform === 'win32' ? 'junction' : 'dir');
    }
    writeFileSync(join(stage, '__exact_paths.json'), JSON.stringify({ app, mounts: [] }));
    writeFileSync(join(stage, '__exact_entry.ts'), `
import { evaluate, callFunction } from '@exact/browser-automation';
import type { PublicKeyCredentialRequestOptionsJSON } from '@exact/passkeys';

export async function title() {
  const { result } = await evaluate({ id: 'guest', expression: 'document.title' });
  // @ts-expect-error An opaque result requires narrowing before property access.
  result.title;
  if (result === 'Exact') return result.toUpperCase();
  if (typeof result === 'string') return result.toLowerCase();
  return null;
}

export const called = callFunction({
  id: 'guest', functionDeclaration: 'value => value',
  arguments: ['text', 42, true, null, [], { title: 'Exact' }],
});

export const request: PublicKeyCredentialRequestOptionsJSON = {
  challenge: 'eA',
  allowCredentials: [{ id: 'eA', type: 'public-key', transports: ['usb'] }],
};
export const timeout: number | undefined = request.timeout;
// @ts-expect-error A known DOM dictionary retains its field types.
request.challenge = 42;
// @ts-expect-error A referenced DOM dictionary retains its field types.
request.allowCredentials![0].id = 42;
`);
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);

    const binding = readFileSync(join(stage, '__exact_modules/exact__passkeys.ts'), 'utf8');
    const dom = readFileSync(join(libraries, 'lib.dom.d.ts'), 'utf8');
    for (const name of ['PublicKeyCredentialRequestOptionsJSON', 'PublicKeyCredentialDescriptorJSON', 'Base64URLString']) {
      const declaration = dom.match(new RegExp(`^(interface ${name}\\b[^}]*\\}|type ${name} = [^;]*;)`, 'm'))?.[1];
      expect(declaration).toBeDefined();
      expect(binding).toContain('export ' + declaration);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

/** An app with `@scratch/cloud` installed, declared by `declaration`, its stage holding `entry`. */
function cloudApp(declaration, entry) {
  const root = mkdtempSync(join(tmpdir(), 'exact-json-list-'));
  const app = join(root, 'app'), stage = join(root, 'stage'), pkg = join(app, 'node_modules/@scratch/cloud');
  mkdirSync(pkg, { recursive: true });
  mkdirSync(stage);
  writeFileSync(join(pkg, 'package.json'), JSON.stringify({ name: '@scratch/cloud', version: '1.0.0', exports: { '.': './exact.json' } }));
  writeFileSync(join(pkg, 'exact.json'), JSON.stringify(declaration));
  writeFileSync(join(stage, '__exact_paths.json'), JSON.stringify({ app, mounts: [] }));
  writeFileSync(join(stage, '__exact_entry.ts'), entry);
  return { root, stage };
}

const cloud = (records) => ({
  kind: 'interface', name: '@scratch/cloud', version: '1.0.0',
  shapes: { CloudRecordWrite: { key: 'string' }, Sent: { echoed: 'option<json<PublicKeyCredentialDescriptorJSON[]>>' } },
  names: [{ send: 'write', request: true, contract: false, answers: 'Sent',
    options: { records, kinds: 'option<json<enum<a|b>[]>>', ids: 'json<PublicKeyCredentialDescriptorJSON>[]' } }],
});

test('json<T> holds a list, an option or an enum of shapes and DOM dictionaries', async () => {
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const { root, stage } = cloudApp(cloud('json<CloudRecordWrite[]>'), `
import { write, type CloudRecordWrite } from '@scratch/cloud';

const records: CloudRecordWrite[] = [{ key: 'a' }];
export const sent = write({ records, kinds: ['a'], ids: [{ id: 'eA', type: 'public-key' }] })
  .then(({ echoed }) => echoed?.[0].id);
// @ts-expect-error A list's element keeps its shape.
write({ records: [{ key: 1 }], ids: [] });
// @ts-expect-error An enum's members are its only values.
write({ records, kinds: ['c'], ids: [] });
`);
  try {
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    const binding = readFileSync(join(stage, '__exact_modules/scratch__cloud.ts'), 'utf8');
    // A declared shape is not also a DOM dictionary; a DOM dictionary in a list or a shape's field is.
    expect(binding).not.toContain('type CloudRecordWrite');
    expect(binding).toContain('export interface PublicKeyCredentialDescriptorJSON');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('json<T> refuses at bake what JSON does not carry, naming what it holds', () => {
  for (const held of ['json<bytes>', 'json<handle<file>>', 'json<json<CloudRecordWrite>>', 'json<lowercase>']) {
    const { root, stage } = cloudApp(cloud(held), '');
    try {
      expect(() => configure(stage, false, parseSync)).toThrow(`@scratch/cloud's exact.json: \`${held}\` is not a JSON value's type: \`json<T>\` holds a shape or DOM dictionary by name`);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test('implementation imports export their own names and call the interface instance', async () => {
  const { root, stage } = cloudApp({ kind: 'interface', name: '@scratch/cloud', shapes: {}, names: [] }, `
import { reading, readingSync, sample, type Reading } from '@scratch/cloud/apple';
import { fast } from '@scratch/cloud-fast';
// @ts-expect-error A platform name is imported from its implementation.
import { reading as sharedReading } from '@scratch/cloud';
export const result: Promise<Reading> = reading();
export const now: Reading = readingSync();
export const sampled = sample({ count: 2 });
export const faster: Promise<Reading> = fast();
// @ts-expect-error A platform option keeps its declared type.
sample({ count: 'two' });
`);
  try {
    const pkg = join(root, 'app/node_modules/@scratch/cloud');
    writeFileSync(join(pkg, 'package.json'), JSON.stringify({ name: '@scratch/cloud', version: '1.0.0', exports: { '.': './exact.json', './apple': './apple/exact.json' } }));
    mkdirSync(join(pkg, 'apple'));
    writeFileSync(join(pkg, 'apple/exact.json'), JSON.stringify({
      kind: 'implementation', implements: '@scratch/cloud@^1.0', platform: 'apple', targets: ['ios'],
      shapes: { Reading: { value: 'number' } }, names: [
        { source: 'reading', answers: 'Reading', sync: { name: 'readingSync' } },
        { send: 'sample', answers: 'Reading', options: { count: 'number' } },
      ],
    }));
    const other = join(root, 'app/node_modules/@scratch/cloud-fast');
    mkdirSync(other);
    writeFileSync(join(other, 'package.json'), JSON.stringify({ name: '@scratch/cloud-fast', version: '1.0.0', exports: './exact.json' }));
    writeFileSync(join(other, 'exact.json'), JSON.stringify({
      kind: 'implementation', implements: '@scratch/cloud@^1.0', targets: ['ios'],
      shapes: { Reading: { value: 'number' } }, names: [{ source: 'fast', answers: 'Reading' }, { source: 'reading', answers: 'Reading', sync: { name: 'readingSync' } }],
    }));
    const app = join(root, 'app'), iface = moduleInterfaces(app).find(i => i.name === '@scratch/cloud');
    expect(implementationFor(app, iface, 'ios').name).toBe('@scratch/cloud/apple');
    writeFileSync(join(app, 'app.json'), JSON.stringify({ implementations: { '@scratch/cloud': { ios: '@scratch/cloud/apple' } } }));
    expect(implementationFor(app, iface, 'ios').name).toBe('@scratch/cloud/apple');
    writeFileSync(join(app, 'app.json'), JSON.stringify({ implementations: { '@scratch/cloud': { ios: '@scratch/cloud-fast' } } }));
    expect(implementationFor(app, iface, 'ios').name).toBe('@scratch/cloud-fast');
    configure(stage, false, parseSync);
    const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
    await check(stage, join(libraries, 'tsc'), libraries);
    const shared = readFileSync(join(stage, '__exact_modules/scratch__cloud.ts'), 'utf8');
    expect(shared).not.toContain('export function reading(');
    expect(readFileSync(join(stage, '__exact_modules/scratch__cloud-fast.ts'), 'utf8')).toContain('extension: \"cloud\"');
    const binding = readFileSync(join(stage, '__exact_modules/scratch__cloud__apple.ts'), 'utf8');
    const js = new Bun.Transpiler({ loader: 'ts' }).transformSync(binding);
    writeFileSync(join(stage, 'binding.mjs'), js);
    const calls = [], previous = globalThis.__exact_module;
    globalThis.__exact_module = async (body) => { calls.push(body); return { value: 7 }; };
    try {
      const bound = await import(join(stage, 'binding.mjs'));
      expect(await bound.reading()).toEqual({ value: 7 });
      expect(await bound.sample({ count: 2 })).toEqual({ value: 7 });
      const otherBinding = readFileSync(join(stage, '__exact_modules/scratch__cloud-fast.ts'), 'utf8');
      writeFileSync(join(stage, 'other-binding.mjs'), new Bun.Transpiler({ loader: 'ts' }).transformSync(otherBinding));
      const other = await import(join(stage, 'other-binding.mjs'));
      expect(await other.reading()).toEqual({ value: 7 });
      expect(calls).toEqual([
        { extension: 'cloud', owner: '@scratch/cloud/apple', source: 'reading', args: {} },
        { extension: 'cloud', owner: '@scratch/cloud/apple', send: 'sample', args: { count: 2 } },
        { extension: 'cloud', owner: '@scratch/cloud-fast', source: 'reading', args: {} },
      ]);
      const priorSync = globalThis.__exact_module_sync;
      globalThis.__exact_module_sync = body => { calls.push(body); return { value: 8 }; };
      try {
        expect(bound.readingSync()).toEqual({ value: 8 });
        expect(other.readingSync()).toEqual({ value: 8 });
        expect(calls.slice(-2)).toEqual([
          { extension: 'cloud', owner: '@scratch/cloud/apple', sync: 'readingSync', args: {} },
          { extension: 'cloud', owner: '@scratch/cloud-fast', sync: 'readingSync', args: {} },
        ]);
      } finally { globalThis.__exact_module_sync = priorSync; }
    } finally { globalThis.__exact_module = previous; }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('a keyed value made by the interface binding keeps its data in an implementation send', async () => {
  const { root, stage } = cloudApp({ kind: 'interface', name: '@scratch/cloud', shapes: { Item: { record: { 'text/plain': 'string' } } }, names: [] }, `
import { Item } from '@scratch/cloud';
import { put } from '@scratch/cloud/apple';
export const sent = put({ item: new Item({ 'text/plain': 'hello' }) });
`);
  try {
    const pkg = join(root, 'app/node_modules/@scratch/cloud');
    writeFileSync(join(pkg, 'package.json'), JSON.stringify({ name: '@scratch/cloud', version: '1.0.0', exports: { '.': './exact.json', './apple': './apple/exact.json' } }));
    mkdirSync(join(pkg, 'apple'));
    writeFileSync(join(pkg, 'apple/exact.json'), JSON.stringify({
      kind: 'implementation', implements: '@scratch/cloud@^1.0', platform: 'apple', targets: ['ios'],
      names: [{ send: 'put', answers: null, options: { item: 'Item' } }],
    }));
    configure(stage, false, parseSync);
    const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
    await check(stage, join(libraries, 'tsc'), libraries);
    const load = async (file, as) => {
      writeFileSync(join(stage, as), new Bun.Transpiler({ loader: 'ts' }).transformSync(readFileSync(join(stage, '__exact_modules', file), 'utf8')));
      return import(join(stage, as));
    };
    const shared = await load('scratch__cloud.ts', 'shared.mjs'), apple = await load('scratch__cloud__apple.ts', 'apple.mjs');
    const calls = [], previous = globalThis.__exact_module;
    globalThis.__exact_module = async (body) => { calls.push(body); return null; };
    try {
      await apple.put({ item: new shared.Item({ 'text/plain': 'hello' }) });
      expect(calls).toEqual([{ extension: 'cloud', owner: '@scratch/cloud/apple', send: 'put', args: { item: { 'text/plain': 'hello' } } }]);
    } finally { globalThis.__exact_module = previous; }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
