import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { rolldown } from 'rolldown';
import { parseSync } from 'rolldown/utils';
import { check, configure, dataBundle, replayCaptured } from '../src/typescript.mjs';

const libraries = join(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');

function linkedApp() {
  const root = realpathSync.native(mkdtempSync(join(tmpdir(), 'exact-package-peers-')));
  const app = join(root, 'app'), stage = join(root, 'stage'), module = join(root, 'module');
  const put = (file, text) => {
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, text);
  };
  const peer = (dir, label) => {
    put(join(dir, 'package.json'), JSON.stringify({
      name: '@scratch/peer', version: '1.4.2', type: 'module',
      exports: {
        './server': { types: './server.d.ts', browser: './server.js', default: './node.js' },
        './values': { types: './values.d.ts', browser: './values.js', default: './node.js' },
      },
    }));
    put(join(dir, 'server.js'), "import { Value } from './values.js'; export const accepts = v => v instanceof Value;\n");
    put(join(dir, 'server.d.ts'), "import { Value } from './values.js'; export declare const accepts: (v: Value) => boolean;\n");
    put(join(dir, 'values.js'), `export class Value { label = '${label}'; }\n`);
    put(join(dir, 'values.d.ts'), `export declare class Value { private brand; label: '${label}'; }\n`);
    put(join(dir, 'node.js'), "throw new Error('the browser export must be selected');\n");
  };
  const peerDir = join(app, 'node_modules/@scratch/peer');
  peer(peerDir, 'app');
  peer(join(module, 'node_modules/@scratch/peer'), 'module');
  const manifest = { name: '@scratch/module', version: '1.0.0', type: 'module', main: './index.js', types: './index.d.ts', peerDependencies: { '@scratch/peer': '^1.4.0' } };
  put(join(module, 'package.json'), JSON.stringify(manifest));
  put(join(module, 'index.js'), "import { Value } from '@scratch/peer/values'; export const value = new Value();\n");
  put(join(module, 'index.d.ts'), "import { Value } from '@scratch/peer/values'; export declare const value: Value;\n");
  symlinkSync(module, join(app, 'node_modules/@scratch/module'), process.platform === 'win32' ? 'junction' : 'dir');
  put(join(app, 'package.json'), '{"name":"peer-app","private":true}');
  const entry = `import { accepts } from '@scratch/peer/server';
import { value } from '@scratch/module';
export const label: 'app' = value.label;
export const same = accepts(value);
`;
  put(join(app, 'app.ts'), entry);
  put(join(stage, 'app.ts'), entry);
  put(join(stage, '__exact_entry.ts'), "export * from './app';\n");
  put(join(stage, '__exact_paths.json'), JSON.stringify({ app, mounts: [] }));
  return { root, app, stage, module, manifest, peerDir, put, close: () => rmSync(root, { recursive: true, force: true }) };
}

async function bundle(stage, replay = false) {
  const options = dataBundle(stage);
  const build = await rolldown({
    input: join(stage, 'app.ts'), logLevel: 'silent',
    ...options, plugins: replay ? [replayCaptured(stage).rolldown] : options.plugins,
  });
  try {
    return (await build.generate({ format: 'esm' })).output[0].code;
  } finally {
    await build.close();
  }
}

test('a linked module checks against the app peer across subpath exports', async () => {
  const fixture = linkedApp();
  const { stage, peerDir } = fixture;
  try {
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    const declarations = readFileSync(join(stage, '__exact_declarations.json'), 'utf8');
    const peers = JSON.parse(declarations).packages.filter(([, , real]) => real.endsWith(join('@scratch', 'peer')));
    expect(peers.map(([, , real]) => real)).toEqual([peerDir]);
    configure(stage, false, parseSync);
    expect(readFileSync(join(stage, '__exact_declarations.json'), 'utf8')).toBe(declarations);
    rmSync(join(fixture.app, 'node_modules'), { recursive: true });
    rmSync(fixture.module, { recursive: true });
    await check(stage, join(libraries, 'tsc'), libraries);
  } finally {
    fixture.close();
  }
});

for (const missing of [true, false]) {
  test(`a linked module refuses an app peer that is ${missing ? 'missing' : 'incompatible'}`, async () => {
    const fixture = linkedApp();
    const { stage, module, manifest, peerDir, put } = fixture;
    try {
      // Only the module names the peer, so the refusal must come from its declaration/runtime import.
      put(join(stage, 'app.ts'), "export { value } from '@scratch/module';\n");
      if (missing) {
        rmSync(peerDir, { recursive: true });
        manifest.peerDependenciesMeta = { '@scratch/peer': { optional: true } };
        put(join(module, 'package.json'), JSON.stringify(manifest));
      } else {
        const path = join(peerDir, 'package.json'), json = JSON.parse(readFileSync(path, 'utf8'));
        put(path, JSON.stringify({ ...json, version: '2.0.0' }));
      }
      const why = '@scratch/module@1.0.0 requires peer @scratch/peer@^1.4.0' +
        (missing ? ', which is not installed in the app' : ', but the app has @scratch/peer@2.0.0');
      configure(stage, false, parseSync);
      await expect(check(stage, join(libraries, 'tsc'), libraries)).rejects.toThrow(why);
      await expect(bundle(stage)).rejects.toThrow(why);
    } finally {
      fixture.close();
    }
  });
}

test('ordinary dependencies still refuse two copies of one package version', async () => {
  const fixture = linkedApp();
  const { stage, module, manifest, root, put } = fixture;
  try {
    delete manifest.peerDependencies;
    manifest.dependencies = { '@scratch/peer': '^1.4.0' };
    put(join(module, 'package.json'), JSON.stringify(manifest));
    const other = join(root, 'node_modules/@scratch/peer');
    mkdirSync(dirname(other), { recursive: true });
    renameSync(join(module, 'node_modules/@scratch/peer'), other);
    expect(() => configure(stage, false, parseSync)).toThrow('package declarations: two copies of @scratch/peer');
    await expect(bundle(stage)).rejects.toThrow('two copies of @scratch/peer@1.4.2');
  } finally {
    fixture.close();
  }
});

test('peer declarations can refer back to their consumer without expanding the capture', async () => {
  const fixture = linkedApp();
  const { stage, peerDir, put } = fixture;
  try {
    const path = join(peerDir, 'package.json'), json = JSON.parse(readFileSync(path, 'utf8'));
    put(path, JSON.stringify({ ...json, peerDependencies: { '@scratch/module': '^1.0.0' } }));
    put(join(peerDir, 'consumer.d.ts'), "export type ModuleValue = typeof import('@scratch/module').value;\n");
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    const { packages } = JSON.parse(readFileSync(join(stage, '__exact_declarations.json'), 'utf8'));
    expect(packages).toHaveLength(2);
  } finally {
    fixture.close();
  }
});

test('a declaration subdirectory cannot shadow its app-owned peer', async () => {
  const fixture = linkedApp();
  const { stage, module, peerDir, put } = fixture;
  try {
    put(join(module, 'index.d.ts'), "export { value } from './sub/index.js';\n");
    put(join(module, 'sub/index.d.ts'), "import { Value } from '@scratch/peer/values'; import '@scratch/other'; export declare const value: Value;\n");
    const nested = join(module, 'sub/node_modules');
    put(join(nested, '@scratch/other/package.json'), '{"name":"@scratch/other","version":"1.0.0","types":"./index.d.ts"}');
    put(join(nested, '@scratch/other/index.d.ts'), "export { Value } from '@scratch/peer/values';\n");
    const peer = JSON.parse(readFileSync(join(peerDir, 'package.json'), 'utf8'));
    put(join(nested, '@scratch/peer/package.json'), JSON.stringify({ ...peer, version: '2.0.0' }));
    put(join(nested, '@scratch/peer/values.d.ts'), "export declare class Value { private brand; label: 'nested'; }\n");
    expect(() => configure(stage, false, parseSync)).toThrow('package declarations: two copies of @scratch/peer');
  } finally {
    fixture.close();
  }
});

test('comments and string types do not require an optional peer', async () => {
  const fixture = linkedApp();
  const { stage, module, manifest, peerDir, put } = fixture;
  try {
    rmSync(peerDir, { recursive: true });
    manifest.peerDependenciesMeta = { '@scratch/peer': { optional: true } };
    put(join(module, 'package.json'), JSON.stringify(manifest));
    put(join(stage, 'app.ts'), "export { value } from '@scratch/module';\n");
    put(join(module, 'index.js'), 'export const value = 1;\n');
    put(join(module, 'index.d.ts'), `// Example: import { Extra } from '@scratch/peer/values';
/* export { Extra } from '@scratch/peer/values'; */
export type Example = "import('@scratch/peer/values').Extra";
export declare const value: 1;
/// <reference types="@scratch/peer" />
`);
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    await bundle(stage);
    for (const declaration of [
      "import type { Value } from /* peer */ '@scratch/peer/values'; export { Value };",
      "export { Value } from '@scratch/peer/values';",
      "export type Value = import('@scratch/peer/values').Value;",
      "import Peer = require('@scratch/peer/values'); export type Value = Peer.Value;",
      '/// <reference types="@scratch/peer" />',
    ]) {
      put(join(module, 'index.d.ts'), declaration);
      configure(stage, false, parseSync);
      await expect(check(stage, join(libraries, 'tsc'), libraries)).rejects.toThrow('@scratch/module@1.0.0 requires peer @scratch/peer@^1.4.0, which is not installed in the app');
    }
  } finally {
    fixture.close();
  }
});

for (const missing of [true, false]) test(`an unused declaration subpath does not require its ${missing ? 'missing' : 'incompatible'} optional peer`, async () => {
  const fixture = linkedApp();
  const { stage, module, manifest, peerDir, put } = fixture;
  try {
    if (missing) rmSync(peerDir, { recursive: true });
    else {
      const path = join(peerDir, 'package.json'), json = JSON.parse(readFileSync(path, 'utf8'));
      put(path, JSON.stringify({ ...json, version: '2.0.0' }));
    }
    manifest.peerDependenciesMeta = { '@scratch/peer': { optional: true } };
    manifest.exports = {
      './server': { types: './server.d.ts', default: './server.js' },
      './extra': { types: './extra.d.ts', default: './extra.js' },
    };
    put(join(module, 'package.json'), JSON.stringify(manifest));
    put(join(module, 'server.js'), 'export const value = 1;\n');
    put(join(module, 'server.d.ts'), "export { value } from './plain.js';\n");
    put(join(module, 'plain.d.ts'), 'export declare const value: 1;\n');
    put(join(module, 'extra.js'), "export { Value } from '@scratch/peer/values';\n");
    put(join(module, 'extra.d.ts'), "export { Value } from './uses-peer.js';\n");
    put(join(module, 'uses-peer.d.ts'), "export type Value = import('@scratch/peer/values').Value;\n");
    put(join(stage, 'app.ts'), "export { value } from '@scratch/module/server';\n");
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    const code = await bundle(stage);
    expect(await bundle(stage, true)).toBe(code);
    // The relative declaration import becomes reachable when this subpath is used.
    put(join(stage, 'app.ts'), "export type { Value } from '@scratch/module/extra';\n");
    configure(stage, false, parseSync);
    await expect(check(stage, join(libraries, 'tsc'), libraries)).rejects.toThrow(
      '@scratch/module@1.0.0 requires peer @scratch/peer@^1.4.0' +
      (missing ? ', which is not installed in the app' : ', but the app has @scratch/peer@2.0.0'));
  } finally {
    fixture.close();
  }
});

test('an installed dependency resolves its peer from its ordinary ancestors', async () => {
  const fixture = linkedApp();
  const { app, stage, module, peerDir, put } = fixture;
  try {
    const parent = join(app, 'node_modules/@scratch/parent');
    const installed = join(parent, 'node_modules/@scratch/module');
    rmSync(join(app, 'node_modules/@scratch/module'));
    mkdirSync(dirname(installed), { recursive: true });
    renameSync(module, installed);
    rmSync(join(installed, 'node_modules'), { recursive: true });
    const nestedPeer = join(parent, 'node_modules/@scratch/peer');
    renameSync(peerDir, nestedPeer);
    put(join(parent, 'package.json'), '{"name":"@scratch/parent","version":"1.0.0","type":"module","main":"index.js","types":"index.d.ts"}');
    const entry = "import { accepts } from '@scratch/peer/server'; import { value } from '@scratch/module'; export { value }; export const same = accepts(value);\n";
    put(join(parent, 'index.js'), entry);
    put(join(parent, 'index.d.ts'), "export { value } from '@scratch/module'; export declare const same: boolean;\n");
    put(join(stage, 'app.ts'), "import { value, same } from '@scratch/parent'; export const label: 'app' = value.label; export { same };\n");
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    const code = await bundle(stage);
    const record = readFileSync(join(stage, '__exact_packages.json'), 'utf8');
    expect(JSON.parse(record).packages.filter(([key]) => key.startsWith('@scratch/peer@')).map(([, real]) => real)).toEqual([nestedPeer]);
    expect(await bundle(stage)).toBe(code);
    rmSync(join(app, 'node_modules'), { recursive: true });
    expect(await bundle(stage, true)).toBe(code);
    const file = join(fixture.root, 'installed.mjs');
    put(file, code);
    const result = await import(file);
    expect(result.same).toBe(true);
    expect(result.label).toBe('app');
  } finally {
    fixture.close();
  }
});

// pnpm and Bun's isolated linker link every package to its place in a store; a consumer's peer sits beside it there.
test('an isolated install resolves a dependency\'s peer beside it in the store', async () => {
  const fixture = linkedApp();
  const { app, stage, module, peerDir, put } = fixture;
  try {
    const store = join(app, 'node_modules/.pnpm');
    const link = (target, path) => {
      mkdirSync(dirname(path), { recursive: true });
      symlinkSync(target, path, process.platform === 'win32' ? 'junction' : 'dir');
    };
    const peer = join(store, '@scratch+peer@1.4.2/node_modules/@scratch/peer');
    const consumer = join(store, '@scratch+module@1.0.0_@scratch+peer@1.4.2/node_modules/@scratch/module');
    const parent = join(store, '@scratch+parent@1.0.0/node_modules/@scratch/parent');
    mkdirSync(dirname(peer), { recursive: true });
    renameSync(peerDir, peer);
    rmSync(join(app, 'node_modules/@scratch'), { recursive: true });
    mkdirSync(dirname(consumer), { recursive: true });
    renameSync(module, consumer);
    rmSync(join(consumer, 'node_modules'), { recursive: true });
    link(peer, join(dirname(dirname(consumer)), '@scratch/peer'));
    link(consumer, join(dirname(dirname(parent)), '@scratch/module'));
    link(peer, join(dirname(dirname(parent)), '@scratch/peer'));
    link(parent, join(app, 'node_modules/@scratch/parent'));
    put(join(parent, 'package.json'), '{"name":"@scratch/parent","version":"1.0.0","type":"module","main":"index.js","types":"index.d.ts","dependencies":{"@scratch/module":"1.0.0","@scratch/peer":"1.4.2"}}');
    put(join(parent, 'index.js'), "import { accepts } from '@scratch/peer/server'; import { value } from '@scratch/module'; export { value }; export const same = accepts(value);\n");
    put(join(parent, 'index.d.ts'), "export { value } from '@scratch/module'; export declare const same: boolean;\n");
    put(join(stage, 'app.ts'), "import { value, same } from '@scratch/parent'; export const label: 'app' = value.label; export { same };\n");
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);
    const code = await bundle(stage);
    const record = JSON.parse(readFileSync(join(stage, '__exact_packages.json'), 'utf8'));
    expect(record.packages.filter(([key]) => key.startsWith('@scratch/peer@')).map(([, real]) => real)).toEqual([peer]);
    const file = join(fixture.root, 'isolated.mjs');
    put(file, code);
    const result = await import(file);
    expect(result.same).toBe(true);
    expect(result.label).toBe('app');
  } finally {
    fixture.close();
  }
});

for (const commonjs of [false, true]) test(`a linked ${commonjs ? 'CommonJS' : 'ESM'} module bundles and replays the app peer across subpath exports`, async () => {
  const fixture = linkedApp();
  const { stage, peerDir, put } = fixture;
  try {
    if (commonjs) {
      put(join(fixture.module, 'package.json'), JSON.stringify({ ...fixture.manifest, main: './index.cjs' }));
      put(join(fixture.module, 'index.cjs'), "exports.value = new (require('@scratch/peer/values').Value)();\n");
    }
    const code = await bundle(stage);
    const record = readFileSync(join(stage, '__exact_packages.json'), 'utf8');
    expect(JSON.parse(record).packages.filter(([key]) => key.startsWith('@scratch/peer@')).map(([, real]) => real)).toEqual([peerDir]);
    expect(JSON.parse(record).edges).toContainEqual([
      `node_modules/.packages/@scratch/module@1.0.0/index.${commonjs ? 'cjs' : 'js'}`,
      '@scratch/peer/values', commonjs ? 'require' : 'import',
      'node_modules/.packages/@scratch/peer@1.4.2/values.js',
    ]);
    expect(await bundle(stage)).toBe(code);
    expect(readFileSync(join(stage, '__exact_packages.json'), 'utf8')).toBe(record);
    rmSync(join(fixture.app, 'node_modules'), { recursive: true });
    rmSync(fixture.module, { recursive: true });
    expect(await bundle(stage, true)).toBe(code);
    const server = await Bun.build({ entrypoints: [join(stage, 'app.ts')], target: 'bun', plugins: [{ name: 'captured', setup: replayCaptured(stage).bun }] });
    expect(server.success).toBe(true);
    for (const [i, built] of [code, await server.outputs[0].text()].entries()) {
      const file = join(fixture.root, `bundle-${i}.mjs`);
      put(file, built);
      const result = await import(file);
      expect(result.label).toBe('app');
      expect(result.same).toBe(true);
    }
  } finally {
    fixture.close();
  }
});
