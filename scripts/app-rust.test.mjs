// `exact new --rust` (scripts/app-rust.mjs): a data crate, a member of the generated workspace, named by app.json's
// rust.data. Compiling and running it is the builds' (bun exact.mjs test web, ios).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createApp } from '../game/new.mjs';
import { ensureShells, rustDataOf } from './app-shells.mjs';
import { authoredWorkspace } from './cargo-check.mjs';

test('exact new --rust writes a data crate that joins the generated workspace, and no host crate', () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-new-rust-'))), dir = resolve(scratch, 'field-log');
  try {
    createApp(dir, { rust: true, title: 'Field Log', with: ['linux'] });
    assert.equal(existsSync(resolve(dir, 'app.ts')), false);
    for (const file of ['data/Cargo.toml', 'data/build.rs', 'data/src/lib.rs']) assert.ok(existsSync(resolve(dir, file)), file);
    for (const file of ['Cargo.toml', 'Cargo.lock', '.cargo', 'apple', 'web', 'linux']) assert.equal(existsSync(resolve(dir, file)), false, file);
    assert.equal(authoredWorkspace(dir), null);
    assert.match(readFileSync(resolve(dir, 'data/Cargo.toml'), 'utf8'), /^workspace = "\.\.\/\.exact\/shells"$/m);
    // The generated workspace takes it as a member, with the host crates that link it.
    const { workspace } = ensureShells(dir);
    const loads = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: resolve(dir, 'data'), encoding: 'utf8' });
    assert.equal(loads.status, 0, loads.stderr);
    const { packages, workspace_root } = JSON.parse(loads.stdout);
    assert.equal(workspace_root, workspace);
    for (const name of ['field-log-data', 'field-log-web', 'field-log-linux']) assert.ok(packages.some(p => p.name === name), name);
    // The source answers the starter's one source, under the type app.json names.
    assert.match(readFileSync(resolve(dir, 'data/src/lib.rs'), 'utf8'), /pub struct FieldLog;[^]*"greeting" =>/);
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    assert.deepEqual(rustDataOf(manifest), { krate: 'field-log-data', ty: 'field_log_data::FieldLog', lib: 'field_log_data', source: 'field_log_data::FieldLog::default()' });
    // What the app tells an agent names its Rust data module, not app.ts or a pattern.
    const notes = readFileSync(resolve(dir, 'AGENTS.md'), 'utf8');
    assert.match(notes, /`data\/src\/lib\.rs` the data module \(Rust\)/);
    assert.doesNotMatch(notes, /app\.ts|pattern <name>|apple\/|web\//);
    assert.match(readFileSync(resolve(dir, 'app.contract'), 'utf8'), /^\/\/ Field Log: the view\. data\/src\/lib\.rs answers/);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 300_000);

test('exact new --rust refuses, before writing, a Snapback starter', () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-new-rust-refusal-'))), dir = resolve(scratch, 'notes');
  try {
    assert.throws(() => createApp(dir, { rust: true, snapback: {} }), /--rust and --snapback are separate starters/);
    assert.equal(existsSync(dir), false);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
});

test('a Rust app\'s source type never takes a name its starter or Rust already uses', async () => {
  const { rustNames, rustListing } = await import('./app-rust.mjs');
  for (const [title, type] of [['Greeting', 'GreetingApp'], ['Value', 'ValueApp'], ['Data Source', 'DataSourceApp'], ['Self', 'SelfApp'], ['Field Log', 'FieldLog'], ['3D', 'App3D'], ['API', 'APIApp'], ['TV', 'TVApp'], ['Q1', 'Q1App']])
  {
    assert.equal(rustNames('x', title).type, type, title);
    // app.json's rust.data names it: every name the scaffold gives it must be one the manifest admits.
    assert.ok(rustDataOf({ rust: { data: { crate: 'x-data', type: `x_data::${type}` } } }), type);
  }
  const listing = rustListing('Created /a\n  bun exact.mjs pattern      start\n  bun exact.mjs web          the web dev loop\n  bun exact.mjs linux        build\n  bun exact.mjs windows --run  on');
  assert.equal(listing, 'Created /a (its data is Rust: data/src/lib.rs)\n  bun exact.mjs web          the web dev loop\n  bun exact.mjs linux        build\n  bun exact.mjs windows --run  on');
});
