import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { applyPattern, createApp } from './new.mjs';
import { dependencyTables, updateManifests, updateScaffold } from './update-manifests.mjs';
import { ensureShells } from '../scripts/app-shells.mjs';
import { pathFrom, resolveApp } from '../scripts/app.mjs';

const root = resolve(import.meta.dir, '..');
const sqlite = 'vendor/ibex/crates/ibex2-sqlite';
const target = 'cfg(target_os = "macos")';
const scopes = ['dependencies', `target.'${target}'.dependencies`, 'workspace.dependencies'];
const dependencies = (doc, scope) => scope === scopes[0] ? doc.dependencies : scope === scopes[1] ? doc.target[target].dependencies : doc.workspace.dependencies;
const write = (file, text) => { mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, text); };
const packageText = (name, path = 'lib.rs') => `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n[lib]\npath = "${path}"\n`;

test('SDK-named app folders keep one crate identity through update, regeneration and renaming', () => {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const parent = mkdtempSync(resolve(root, 'target/app-identity-'));
  try {
    for (const [index, folder, title, name] of [
      [0, 'exact', 'Everbloom', 'everbloom'],
      [1, 'exact-js', 'Éver Bloom!', 'ever-bloom'],
      [2, 'exact', 'Exact_JS', 'exact-js-app'],
      [3, 'exact', '123 Garden', 'app-123-garden'],
      [4, 'exact', '🌱', 'app'],
      [5, 'exact', undefined, 'exact-app'],
    ]) {
      let dir = resolve(parent, String(index), folder);
      createApp(dir, { title, with: ['ios', 'linux'] });
      const pkg = () => JSON.parse(readFileSync(resolve(dir, 'package.json'), 'utf8'));
      assert.equal(pkg().name, name);
      assert.match(applyPattern(dir, 'one-screen').said, /^one-screen: wrote/);
      for (let pass = 0; pass < 3; pass++) {
        if (pass === 1) {
          const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
          manifest.name = manifest.app.name = 'Changed Display Name';
          writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
          const moved = resolve(parent, String(index), 'Exact Feature');
          renameSync(dir, moved);
          dir = moved;
        }
        createApp(dir, { update: true });
        assert.match(readFileSync(resolve(dir, 'exact.mjs'), 'utf8'), new RegExp(`name: "${name}",`));
        const { workspace, crates } = ensureShells(dir);
        assert.deepEqual(crates, ['web', 'apple', 'linux']);
        for (const kind of crates) {
          const doc = Bun.TOML.parse(readFileSync(resolve(workspace, kind, 'Cargo.toml'), 'utf8'));
          assert.equal(doc.package.name, `${name}-${kind}`);
          assert.equal(resolveApp(dir).crate(kind), doc.package.name);
        }
        assert.equal(ensureShells(dir).wrote, false);
        rmSync(workspace, { recursive: true, force: true });
      }
      assert.equal(pkg().name, name);
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60_000);

test('authored crate names win over the package name and an SDK-named directory', () => {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const parent = mkdtempSync(resolve(root, 'target/app-identity-'));
  try {
    const made = resolve(parent, 'original'), moved = resolve(parent, 'exact');
    createApp(made, { with: ['ios'] });
    const { workspace, crates } = ensureShells(made);
    for (const entry of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', ...crates]) {
      cpSync(resolve(workspace, entry), resolve(made, entry), { recursive: true });
    }
    rmSync(workspace, { recursive: true, force: true });
    for (const kind of crates) {
      const file = resolve(made, kind, 'Cargo.toml');
      writeFileSync(file, readFileSync(file, 'utf8').replace(`original-${kind}`, `authored-${kind}`));
    }
    renameSync(made, moved);
    for (let pass = 0; pass < 2; pass++) {
      createApp(moved, { update: true, with: ['linux', 'android'] });
      assert.match(readFileSync(resolve(moved, 'exact.mjs'), 'utf8'), /name: "authored",/);
      for (const kind of ['web', 'apple', 'linux', 'android']) {
        const doc = Bun.TOML.parse(readFileSync(resolve(moved, kind, 'Cargo.toml'), 'utf8'));
        assert.equal(doc.package.name, `authored-${kind}`);
        assert.equal(resolveApp(moved).crate(kind), doc.package.name);
      }
    }
    assert.equal(JSON.parse(readFileSync(resolve(moved, 'package.json'), 'utf8')).name, 'original');
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60_000);

function declaration(key, form, scope, path) {
  if (form === 'inline') return `${key} = { path = ${JSON.stringify(path)} }\n`;
  if (form === 'workspace') return `${key}.workspace = true\n`;
  if (form === 'dotted') return `${key}.path = ${JSON.stringify(path)}\n`;
  return `[${scope}.${key}]\npath = ${JSON.stringify(path)}\n`;
}

function tableText(scope, parent, host, provider, sdk, from) {
  const entries = [['exact-apple', host, 'host/apple'], ['ibex2-sqlite', provider, sqlite]].filter(([, form]) => form !== 'missing');
  if (parent === 'inline') {
    const prefix = scope === scopes[0] ? '' : scope === scopes[1] ? `[target.'${target}']\n` : '[workspace]\n';
    return prefix + 'dependencies = { ' + entries.map(([key, form, path]) => declaration(key, form, scope, pathFrom(from, resolve(sdk, path))).trim()).join(', ') + ' }\n';
  }
  const ordinary = entries.filter(([, form]) => form !== 'subtable');
  const children = entries.filter(([, form]) => form === 'subtable');
  return (parent === 'headerless' ? '' : `[${scope}]\n`) + [...ordinary, ...children]
    .map(([key, form, path]) => declaration(key, form, scope, pathFrom(from, resolve(sdk, path)))).join('');
}

// Inline parents cannot contain headers. Their expressible dependency forms are inline values and dotted keys.
// Headerless cases retain the child-table-only manifests that the updater itself writes.
const cases = [];
for (const scope of scopes) for (const parent of ['header', 'inline']) {
  const forms = ['inline', 'workspace', 'dotted', ...(parent === 'header' ? ['subtable'] : [])];
  for (const host of forms) for (const provider of ['missing', ...forms]) cases.push({ scope, parent, host, provider });
}
for (const scope of scopes) for (const provider of ['missing', 'subtable']) cases.push({ scope, parent: 'headerless', host: 'subtable', provider });

test('update adds the SQLite provider across dependency and parent table forms', () => {
  const parentDir = mkdtempSync(resolve(tmpdir(), 'exact-sqlite-matrix-')), failures = [];
  let operations = 0, refusals = 0, loads = 0;
  const check = (label, body) => { try { body(); } catch (error) { failures.push(`${label}: ${error.stack}`); } };
  try {
    for (const [index, c] of cases.entries()) {
      const label = `${c.scope}/${c.parent}/${c.host}/${c.provider}`;
      check(label, () => {
        const fixture = resolve(parentDir, `case-${index}`), app = resolve(fixture, 'app');
        let sdk = resolve(fixture, 'sdk-before');
        const crates = () => new Map([['exact-apple', resolve(sdk, 'host/apple')], ['ibex2-sqlite', resolve(sdk, sqlite)]]);
        write(resolve(sdk, 'Cargo.toml'), '[workspace]\nmembers = ["host/apple", "vendor/ibex/crates/ibex2-sqlite"]\nresolver = "2"\n');
        for (const [name, path] of crates()) { write(resolve(path, 'Cargo.toml'), packageText(name)); write(resolve(path, 'lib.rs'), ''); }
        const workspace = c.scope === 'workspace.dependencies';
        const file = resolve(app, workspace ? 'Cargo.toml' : 'member/Cargo.toml');
        mkdirSync(dirname(file), { recursive: true });
        const before = tableText(c.scope, c.parent, c.host, c.provider, sdk, dirname(file));
        const workspaceHeader = '[workspace]\nmembers = ["member"]\nresolver = "2"\n';
        const member = resolve(app, 'member/Cargo.toml'), rootFile = resolve(app, 'Cargo.toml');
        write(member, !workspace && c.parent === 'inline' && c.scope === 'dependencies'
          ? before + packageText('matrix-member') : packageText('matrix-member') + (workspace ? '' : before));
        write(resolve(app, 'member/lib.rs'), '');
        if (workspace) {
          // An inline workspace dependency table shares the workspace's header with members and resolver.
          write(rootFile, c.parent === 'inline' ? workspaceHeader + before.replace('[workspace]\n', '') : workspaceHeader + before);
        } else {
          write(rootFile, workspaceHeader + `[workspace.dependencies]\nexact-apple = { path = ${JSON.stringify(pathFrom(app, resolve(sdk, 'host/apple')))} }\n`);
        }
        const refused = c.parent === 'inline' && c.provider === 'missing';
        const oldSdk = sdk;
        let previous;
        for (const operation of ['update', 'update again', 'move SDK and update']) {
          if (operation === 'move SDK and update') { sdk = resolve(fixture, 'sdk-after'); renameSync(oldSdk, sdk); }
          check(`${label}: ${operation}`, () => {
            operations++;
            const original = readFileSync(file, 'utf8');
            let error;
            // Exercise the same checked file writer as exact update, including refusal aggregation and persistence.
            try { updateManifests(app, crates()); } catch (caught) { error = caught; }
            const text = readFileSync(file, 'utf8'), doc = Bun.TOML.parse(text), deps = dependencies(doc, c.scope);
            if (refused) {
              refusals++;
              assert.ok(error, 'inline insertion must refuse');
              assert.equal(text, original, 'a refused manifest must remain byte for byte unchanged');
              assert.ok(error.message.includes(file), error.message);
              assert.match(error.message, /add: ibex2-sqlite = \{ (?:path = "[^"]+"|workspace = true) \}/);
            } else {
              assert.ifError(error);
              assert.ok(deps['ibex2-sqlite'], 'native host has its provider in the same table');
              if (!workspace && c.provider === 'missing') assert.deepEqual(deps['ibex2-sqlite'], { workspace: true });
              if (operation === 'update again') assert.equal(text, previous, 'second update is byte for byte idempotent');
              if (operation === 'update' && c.provider !== 'missing') assert.equal(text, original, 'existing declarations keep their authored form');
              for (const manifest of [rootFile, member]) {
                const contents = readFileSync(manifest, 'utf8');
                const parsed = Bun.TOML.parse(contents);
                for (const [, table] of dependencyTables(parsed)) for (const [name, dep] of Object.entries(table)) {
                  if (dep.path) assert.equal(resolve(dirname(manifest), dep.path), crates().get(name), `${manifest}: ${name}`);
                }
                if (operation === 'move SDK and update') assert.ok(!contents.includes('sdk-before'), contents);
              }
            }
            // Attempt every load, including authored workspace inheritance and refused paths after the SDK moved.
            // A refusal can still leave a loadable workspace when the host inherits its path from the root.
            const loaded = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: app, encoding: 'utf8' });
            if (loaded.status === 0) loads++;
            else {
              const authoredInheritance = workspace && (c.host === 'workspace' || c.provider === 'workspace');
              const retainedOldPath = refused && operation === 'move SDK and update' && Object.values(deps).some(dep => dep.path);
              assert.ok(authoredInheritance || retainedOldPath, loaded.stderr);
              assert.match(loaded.stderr, /workspace|dependency|sdk-before/);
            }
            previous = text;
          });
        }
      });
    }
    console.log(`SQLite manifest matrix: ${cases.length} cases, ${operations} operations, ${refusals} refusals, ${loads} offline Cargo loads`);
    assert.deepEqual(failures, [], failures.join('\n\n'));
    assert.equal(operations, 306);
    assert.equal(refusals, 27);
  } finally { rmSync(parentDir, { recursive: true, force: true }); }
}, 120_000);

test('update adds the SQLite provider to existing authored native shells and follows a moved SDK', () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-sqlite-update-')), dir = resolve(parent, 'notes');
  try {
    createApp(dir, { with: ['macos', 'linux', 'android', 'windows'] });
    const { crates: generated, workspace } = ensureShells(dir);
    for (const entry of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', ...generated]) cpSync(resolve(workspace, entry), resolve(dir, entry), { recursive: true });
    rmSync(workspace, { recursive: true, force: true });
    const crates = generated.filter(kind => kind !== 'web');
    for (const kind of crates) {
      const file = resolve(dir, kind, 'Cargo.toml');
      writeFileSync(file, readFileSync(file, 'utf8').replace(/^ibex2-sqlite = .*\n/m, '')
        .replace(/^(\[build-dependencies\]\n)/m, '$1contract = { path = "<exact2>/contract/cli" }\n'));
    }
    createApp(dir, { update: true });
    for (const kind of crates) {
      const file = resolve(dir, kind, 'Cargo.toml');
      const doc = Bun.TOML.parse(readFileSync(file, 'utf8')), provider = (doc.dependencies ?? Object.values(doc.target)[0].dependencies)['ibex2-sqlite'];
      assert.equal(provider.path, pathFrom(resolve(dir, kind), resolve(import.meta.dir, '../vendor/ibex/crates/ibex2-sqlite')), kind);
      assert.equal(Bun.TOML.parse(readFileSync(file, 'utf8'))['build-dependencies'].contract.path,
        pathFrom(resolve(dir, kind), resolve(import.meta.dir, '../contract/cli')), kind);
    }
    // Keep the appended provider child tables. Only the path text changes when the previous SDK disappears.
    for (const kind of generated) {
      const file = resolve(dir, kind, 'Cargo.toml');
      const prefix = pathFrom(resolve(dir, kind), root);
      writeFileSync(file, readFileSync(file, 'utf8').replaceAll(prefix + '/host/', '/nowhere/old-sdk/host/').replaceAll(prefix + '/vendor/', '/nowhere/old-sdk/vendor/').replaceAll(prefix + '/logic', '/nowhere/old-sdk/logic'));
    }
    createApp(dir, { update: true });
    const manifests = crates.map(kind => readFileSync(resolve(dir, kind, 'Cargo.toml'), 'utf8'));
    for (const text of manifests) assert.ok(!text.includes('/nowhere'));
    createApp(dir, { update: true });
    assert.deepEqual(crates.map(kind => readFileSync(resolve(dir, kind, 'Cargo.toml'), 'utf8')), manifests);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('update adds the SQLite provider only after checking the complete root manifest', () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-sqlite-refusal-')), dir = resolve(parent, 'notes');
  try {
    createApp(dir);
    const file = resolve(dir, 'Cargo.toml');
    const original = `[workspace]\nmembers = []\ndependencies = { exact-apple = { path = ${JSON.stringify(pathFrom(dir, resolve(root, 'host/apple')))} } }\n`;
    write(file, original);
    const result = spawnSync(process.execPath, [resolve(dir, 'exact.mjs'), 'update'], { cwd: dir, env: process.env, encoding: 'utf8' });
    assert.notEqual(result.status, 0, result.stdout);
    assert.ok(result.stderr.includes(file), result.stderr);
    assert.match(result.stderr, /inside workspace.dependencies, add: ibex2-sqlite = \{ path = "[^"]+" \}/);
    assert.equal(readFileSync(file, 'utf8'), original, 'refusal also leaves the pending patch-table edit unwritten');
    assert.deepEqual(Bun.TOML.parse(readFileSync(file, 'utf8')), Bun.TOML.parse(original));
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('update refreshes an older Android scaffold, absolute packages and the named checkout', () => {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const parent = mkdtempSync(resolve(root, 'target/update-scaffold-')), dir = resolve(parent, 'notes');
  try {
    createApp(dir, { with: ['android'] });
    const { workspace, crates } = ensureShells(dir);
    for (const entry of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', ...crates]) cpSync(resolve(workspace, entry), resolve(dir, entry), { recursive: true });
    rmSync(workspace, { recursive: true, force: true });
    const manifest = resolve(dir, 'android/Cargo.toml'), source = resolve(dir, 'android/src/lib.rs');
    writeFileSync(manifest, readFileSync(manifest, 'utf8').replace('exact-painted =', 'exact-linux =').replace('/host/painted', '/host/linux'));
    writeFileSync(source, readFileSync(source, 'utf8').replace('exact_painted::canvas_jni!', 'exact_linux::canvas_jni!'));
    const pkg = resolve(dir, 'package.json');
    const original = '{"name":"notes","large":9007199254740993,"dependencies":{"@exact/motion":"file:/old/sdk/packages/motion"},"devDependencies":{"@exact/device":"file:/old/sdk/packages/device"},"optionalDependencies":{"@exact/unknown":"file:/old/sdk/packages/unknown"},"overrides":{"@exact/motion":"file:/old/sdk/packages/motion"}}\n';
    writeFileSync(pkg, original);
    const launcher = resolve(dir, 'exact.mjs');
    writeFileSync(launcher, readFileSync(launcher, 'utf8').replace(/(const path = .*\?\? )"[^"]+";/, '$1"../old-sdk";'));
    const said = createApp(dir, { update: true });
    assert.match(readFileSync(source, 'utf8'), /exact_painted::canvas_jni!/);
    const deps = Object.values(Bun.TOML.parse(readFileSync(manifest, 'utf8')).target)[0].dependencies;
    assert.ok(deps['exact-linux'], 'generated includes may still use exact-linux');
    assert.equal(resolve(dir, 'android', deps['exact-painted'].path), resolve(root, 'host/painted'));
    assert.equal(readFileSync(pkg, 'utf8'), original.replace('"dependencies":{"@exact/motion":"file:/old/sdk/packages/motion"}', `"dependencies":{"@exact/motion":${JSON.stringify('file:' + pathFrom(dir, resolve(root, 'packages/motion')))}}`)
      .replace('"devDependencies":{"@exact/device":"file:/old/sdk/packages/device"}', `"devDependencies":{"@exact/device":${JSON.stringify('file:' + pathFrom(dir, resolve(root, 'packages/device')))}}`));
    assert.match(said, /exact_painted::canvas_jni!/);
    assert.match(said, /package.json.*bun install/);
    assert.match(said, /exact.mjs now names.*it named.*old-sdk/);
    const again = createApp(dir, { update: true });
    assert.doesNotMatch(again, /canvas_jni|package.json.*bun install|exact.mjs now names/);
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60_000);

test('update explains authored Android dependencies before changing either file', () => {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const parent = mkdtempSync(resolve(root, 'target/update-android-'));
  try {
    const forms = [
      '[dependencies.exact-linux]\npath = "<exact2>/host/linux"\n',
      '[dependencies]\nexact-linux = { workspace = true }\n',
      '[dependencies]\nlinux-host = { package = "exact-linux", path = "<exact2>/host/linux" }\n',
      '[dependencies]\nexact-linux = { path = "<exact2>/host/linux" }\n[dependencies.exact-painted]\npath = "<exact2>/host/painted"\n',
    ];
    for (const [index, dependencies] of forms.entries()) {
      const dir = resolve(parent, `notes-${index}`);
      createApp(dir);
      write(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers = ["android"]\n[workspace.dependencies]\nexact-linux = { path = "<exact2>/host/linux" }\n');
      const manifest = resolve(dir, 'android/Cargo.toml'), source = resolve(dir, 'android/src/lib.rs');
      write(manifest, packageText(`notes-${index}-android`, 'src/lib.rs') + dependencies);
      write(source, 'exact_linux::canvas_jni!(AppData);\n');
      const said = createApp(dir, { update: true });
      const loaded = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: dir, encoding: 'utf8' });
      assert.equal(loaded.status, 0, loaded.stderr);
      assert.equal(JSON.parse(loaded.stdout).packages[0].targets.find(t => t.kind.includes('lib')).src_path, source);
      assert.equal(readFileSync(source, 'utf8'), 'exact_linux::canvas_jni!(AppData);\n');
      assert.match(said, /android\/src\/lib.rs.*change.*exact_linux::canvas_jni!.*exact_painted::canvas_jni!/);
      assert.match(said, /android\/Cargo.toml.*exact-painted/);
      const parsed = Bun.TOML.parse(readFileSync(manifest, 'utf8'));
      assert.equal(Boolean(parsed.dependencies['exact-painted']), index === 3);
    }
    const dir = resolve(parent, 'other-uses');
    createApp(dir);
    write(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers = ["android"]\n');
    write(resolve(dir, 'android/Cargo.toml'), packageText('other-uses-android', 'src/lib.rs') + '[dependencies]\nexact-linux = { path = "<exact2>/host/linux" }\n');
    write(resolve(dir, 'android/src/lib.rs'), 'exact_linux::canvas_jni!(AppData);\nmod other;\n');
    write(resolve(dir, 'android/src/other.rs'), 'use exact_linux::run_empty;\n');
    createApp(dir, { update: true });
    const deps = Bun.TOML.parse(readFileSync(resolve(dir, 'android/Cargo.toml'), 'utf8')).dependencies;
    assert.ok(deps['exact-linux'] && deps['exact-painted']);
    assert.match(readFileSync(resolve(dir, 'android/src/lib.rs'), 'utf8'), /exact_painted::canvas_jni!/);
    assert.equal(readFileSync(resolve(dir, 'android/src/other.rs'), 'utf8'), 'use exact_linux::run_empty;\n');
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60_000);

test('Android canvas migration retains dependencies used outside src or by uncertain sources', () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-android-sources-'));
  const cases = [
    ['example', '', 'examples/headless.rs', 'use exact_linux::run_empty;\n'],
    ['test', '', 'tests/headless.rs', 'use exact_linux::run_empty;\n'],
    ['bench', '', 'benches/headless.rs', 'use exact_linux::run_empty;\n'],
    ['bin', '\n[[bin]]\nname = "headless"\npath = "tools/main.rs"\n', 'tools/main.rs', 'use exact_linux::run_empty;\n'],
    ['external-target', '\n[[example]]\nname = "external"\npath = "../shared/main.rs"\n', '../shared/main.rs', 'use exact_linux::run_empty;\n'],
    ['external-module', '', '../shared/other.rs', 'use exact_linux::run_empty;\n', '#[path = "../../shared/other.rs"]\nmod other;\n'],
    ['generated-module', '', 'build.rs', 'fn main() {}\n', 'include!(concat!(env!("OUT_DIR"), "/bindings.rs"));\n'],
    ['unused', '', 'src/other.rs', '', '', false],
  ];
  try {
    for (const [name, targets, file, contents, extra = '', keep = true] of cases) {
      const dir = resolve(parent, name), android = resolve(dir, 'android');
      write(resolve(android, 'Cargo.toml'), packageText(name, 'src/lib.rs') + '[dependencies]\nexact-linux = { path = "/old/sdk/host/linux" }\n' + targets);
      write(resolve(android, 'src/lib.rs'), 'exact_linux::canvas_jni!(AppData);\n' + extra);
      write(resolve(android, file), contents);
      updateScaffold(dir);
      const deps = Bun.TOML.parse(readFileSync(resolve(android, 'Cargo.toml'), 'utf8')).dependencies;
      assert.equal(Boolean(deps['exact-linux']), keep, name);
      assert.ok(deps['exact-painted'], name);
      assert.match(readFileSync(resolve(android, 'src/lib.rs'), 'utf8'), /exact_painted::canvas_jni!/);
      assert.equal(readFileSync(resolve(android, file), 'utf8'), contents);
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('update reports checkout changes for the historical EXACT2 launcher', () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-old-launcher-')), dir = resolve(parent, 'notes');
  try {
    createApp(dir);
    write(resolve(dir, 'exact.mjs'), 'import { resolve } from "node:path";\nconst EXACT2 = resolve(import.meta.dir, process.env.EXACT2 ?? "../sdk");\n');
    const said = createApp(dir, { update: true });
    const next = /const path = .*\?\? ("(?:\\.|[^"\\])*");/.exec(readFileSync(resolve(dir, 'exact.mjs'), 'utf8'))[1];
    assert.ok(said.includes(`exact.mjs now names the Exact checkout ${JSON.parse(next)} (it named ../sdk;`), said);
    assert.doesNotMatch(createApp(dir, { update: true }), /exact.mjs now names/);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('package path updates accept JSONC and leave unparseable files while updating Cargo', () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-jsonc-update-'));
  const spec = 'file:/old/sdk/packages/motion';
  const cases = [
    ['trailing', `{"dependencies":{"@exact/motion":"${spec}",},}\n`, true],
    ['comments', `{// a lone " and a fake "${spec}"\n"dependencies":{/* " */"@exact/motion":"${spec}"},"overrides":{"@exact/motion":"${spec}"}}\n`, true],
    ['invalid', '{"dependencies": {', false],
  ];
  try {
    for (const [name, original, valid] of cases) {
      const dir = resolve(parent, name);
      createApp(dir);
      write(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers = ["android"]\n');
      write(resolve(dir, 'android/Cargo.toml'), packageText(`${name}-android`, 'src/lib.rs') + '[dependencies]\nexact-painted = { path = "/old/sdk/host/painted" }\n');
      write(resolve(dir, 'android/src/lib.rs'), '');
      write(resolve(dir, 'package.json'), original);
      const said = createApp(dir, { update: true });
      const expected = valid ? original.replace(`"@exact/motion":"${spec}"`, `"@exact/motion":${JSON.stringify('file:' + pathFrom(dir, resolve(root, 'packages/motion')))}`) : original;
      assert.equal(readFileSync(resolve(dir, 'package.json'), 'utf8'), expected, name);
      assert.match(said, valid ? /package.json.*bun install/ : /package.json.*left unchanged/);
      const deps = Bun.TOML.parse(readFileSync(resolve(dir, 'android/Cargo.toml'), 'utf8')).dependencies;
      assert.equal(resolve(dir, 'android', deps['exact-painted'].path), resolve(root, 'host/painted'));
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60_000);
