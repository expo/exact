// One Cargo graph names the checkout one way (scripts/paths.mjs `cargoCheckout`, LLP 1086 D5): expo/exact#1382.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { cargoCheckout, checkoutPathDependencies, pathFrom, slashRelative } from './paths.mjs';

const ROOT = realpathSync(resolve(import.meta.dir, '..'));

test('a data crate that names exact2 through the app\'s link is refused, update makes it inherit, and the graph holds one copy', async () => {
  const { createApp } = await import('../game/new.mjs');
  const { ensureShells } = await import('./app-shells.mjs');
  const { updateScaffold } = await import('../game/update-manifests.mjs');
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-checkout-identity-'))), dir = resolve(scratch, 'authapp');
  try {
    createApp(dir, { with: ['ios'] });
    const link = resolve(dir, '.exact/checkout');
    assert.equal(realpathSync(link), ROOT);
    assert.equal(cargoCheckout(dir, true), ROOT);
    assert.equal(cargoCheckout(dir, false), link);
    assert.equal(cargoCheckout(scratch, false), ROOT);
    // A crate of the app's own, reached by path, is not exact2's.
    for (const at of ['data/src', 'data/parse/src']) mkdirSync(resolve(dir, at), { recursive: true });
    writeFileSync(resolve(dir, 'data/parse/Cargo.toml'), '[package]\nname = "authapp-parse"\nversion = "0.1.0"\nedition = "2021"\n');
    writeFileSync(resolve(dir, 'data/parse/src/lib.rs'), '');
    const manifest = resolve(dir, 'data/Cargo.toml');
    writeFileSync(manifest, `[package]
name = "authapp-data"
version.workspace = true
edition.workspace = true
workspace = "../.exact/shells"

[dependencies]
exact-runner.workspace = true
authapp-parse = { path = "parse" }
exact-canvas = { path = "../.exact/checkout/canvas", default-features = false } # the reporter's line

[dev-dependencies.exact-plan]
path = "../.exact/checkout/plan"

[build_dependencies]
shapes = { package = "contract", path = "../.exact/checkout/contract/cli" }
`);
    writeFileSync(resolve(dir, 'data/src/lib.rs'), 'pub struct Authapp;\n');
    const app = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...app, rust: { data: { crate: 'authapp-data', type: 'authapp_data::Authapp' } } }));
    assert.deepEqual(checkoutPathDependencies(manifest).map(({ keys, key, dir }) => [keys.join('.'), key, dir]),
      [['dependencies', 'exact-canvas', resolve(ROOT, 'canvas')], ['build_dependencies', 'shapes', resolve(ROOT, 'contract/cli')], ['dev-dependencies', 'exact-plan', resolve(ROOT, 'plan')]]);
    const refusal = () => { try { ensureShells(dir); } catch (error) { return error.message.split('\n'); } return []; };
    const [canvas, shapes, plan] = refusal();
    assert.match(canvas, /data[\\/]Cargo\.toml: \[dependencies\] exact-canvas names exact2's exact-canvas through the app's \.exact\/checkout link, a second copy of the one the generated workspace names; take it from that workspace: exact-canvas\.workspace = true \(`bun exact\.mjs update` rewrites it\)$/);
    assert.match(plan, /\[dev-dependencies\] exact-plan names exact2's exact-plan through the app's/);
    // A renamed dependency is offered only under exact2's name, which update cannot choose for the author's code.
    assert.match(shapes, /\[build_dependencies\] shapes names exact2's contract through the app's [^]*: contract\.workspace = true, without package or path \(Rust then names it contract\)$/);

    const noted = () => updateScaffold(dir).filter(note => /data[\\/]/.test(note));
    assert.deepEqual(noted().map(note => note.replaceAll('\\', '/')), ['data/Cargo.toml takes exact2\'s crates from its workspace']);
    assert.match(refusal().join('\n'), /^[^\n]*\[build_dependencies\] shapes[^\n]*$/);
    writeFileSync(manifest, readFileSync(manifest, 'utf8').replace(/^shapes = .*$/m, 'contract.workspace = true'));
    const written = readFileSync(manifest, 'utf8');
    assert.match(written, /^exact-canvas = \{ workspace = true, default-features = false \} # the reporter's line$/m);
    assert.match(written, /^\[dev-dependencies\.exact-plan\]\nworkspace = true$/m);
    assert.match(written, /^authapp-parse = \{ path = "parse" \}$/m);
    assert.deepEqual(noted(), []);

    const { workspace } = ensureShells(dir);
    const shell = readFileSync(resolve(workspace, 'Cargo.toml'), 'utf8');
    // Cargo reads default-features only from the workspace's entry, so the member's choice is written there.
    for (const [name, at, rest] of [['exact-canvas', 'canvas', ', default-features = false'], ['exact-plan', 'plan', '']]) {
      const path = new RegExp(`^${name} = \\{ path = "([^"]+)"${rest} \\}$`, 'm').exec(shell)?.[1];
      assert.ok(path && !path.includes('.exact/checkout') && realpathSync(resolve(workspace, path)) === resolve(ROOT, at), shell);
    }
    // Resolved whole, the graph has one exact-canvas, at the checkout's real path.
    const metadata = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], { cwd: workspace, encoding: 'utf8', maxBuffer: 1 << 28 });
    assert.equal(metadata.status, 0, metadata.stderr);
    const copies = JSON.parse(metadata.stdout).packages.filter(p => p.name === 'exact-canvas').map(p => p.manifest_path);
    assert.deepEqual(copies, [resolve(ROOT, 'canvas/Cargo.toml')]);

    // A crate a member reaches by path cannot inherit, so through the link it is refused with that said.
    writeFileSync(resolve(dir, 'data/parse/Cargo.toml'), '[package]\nname = "authapp-parse"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nexact-plan = { path = "../../.exact/checkout/plan" }\n');
    assert.throws(() => ensureShells(dir), /parse[\\/]Cargo\.toml: \[dependencies\] exact-plan names exact2's exact-plan through the app's \.exact\/checkout link[^\n]*; only a crate beside app\.json that joins the workspace can take it from there/);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}, 180_000);

test('an app inside the checkout names it by relative path, as the checkout\'s own workspace does, though it has a link', () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const app = mkdtempSync(resolve(ROOT, 'target/exact-checkout-identity-'));
  try {
    mkdirSync(resolve(app, '.exact'));
    symlinkSync(ROOT, resolve(app, '.exact/checkout'), process.platform === 'win32' ? 'junction' : 'dir');
    mkdirSync(resolve(app, 'apple'));
    writeFileSync(resolve(app, 'app.json'), '{}');
    const written = pathFrom(resolve(app, 'apple'), resolve(ROOT, 'host/apple'));
    assert.equal(written, slashRelative(resolve(app, 'apple'), resolve(ROOT, 'host/apple')));
    assert.equal(cargoCheckout(app, false), ROOT);
  } finally {
    rmSync(app, { recursive: true, force: true });
  }
});
