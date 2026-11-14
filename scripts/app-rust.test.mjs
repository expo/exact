// `exact new --rust` (scripts/app-rust.mjs): the app's own workspace, resolved and loadable, whose web bake the module
// build can copy. Compiling and running it is the web and Apple builds' (bun exact.mjs test web, ios).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createApp } from '../game/new.mjs';
import { bakedRustSource } from '../host/web-js/module.mjs';
import { authoredWorkspace } from './cargo-check.mjs';

test('exact new --rust writes a data crate and host crates in a workspace of the app\'s own, with every path resolved', () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-new-rust-'))), dir = resolve(scratch, 'field-log');
  try {
    createApp(dir, { rust: true, title: 'Field Log' });
    assert.equal(existsSync(resolve(dir, 'app.ts')), false);
    const files = ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml', 'data/build.rs', 'data/src/lib.rs', 'apple/build.rs', 'apple/src/lib.rs', 'web/build.rs', 'web/src/lib.rs', 'web/src/bin/field-log-dev.rs'];
    for (const file of files) assert.ok(existsSync(resolve(dir, file)), file);
    assert.equal(authoredWorkspace(dir), dir);
    for (const manifest of ['Cargo.toml', 'data/Cargo.toml', 'apple/Cargo.toml', 'web/Cargo.toml'])
      assert.doesNotMatch(readFileSync(resolve(dir, manifest), 'utf8'), /path = "<exact2>/, manifest);
    const loads = spawnSync('cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1'], { cwd: dir, encoding: 'utf8' });
    assert.equal(loads.status, 0, loads.stderr);
    assert.deepEqual(JSON.parse(loads.stdout).packages.map(p => p.name).sort(), ['field-log-apple', 'field-log-data', 'field-log-web']);
    // The source answers the starter's one source, under the type the bakes name.
    assert.match(readFileSync(resolve(dir, 'data/src/lib.rs'), 'utf8'), /pub struct FieldLog;[^]*"greeting" =>/);
    assert.deepEqual(bakedRustSource(dir), { call: 'contract::bake', ty: 'field_log_data::FieldLog', expr: 'field_log_data::FieldLog::default()' });
    // What the app tells an agent names its Rust data module, not app.ts or a pattern.
    const notes = readFileSync(resolve(dir, 'AGENTS.md'), 'utf8');
    assert.match(notes, /`data\/src\/lib\.rs` the data module \(Rust/);
    assert.doesNotMatch(notes, /app\.ts|pattern <name>/);
    assert.match(readFileSync(resolve(dir, 'app.contract'), 'utf8'), /^\/\/ Field Log: the view\. data\/src\/lib\.rs answers/);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 60_000);

test('exact new --rust refuses, before writing, a platform its scaffold has no crate for and a Snapback starter', () => {
  const scratch = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-new-rust-refusal-'))), dir = resolve(scratch, 'notes');
  try {
    assert.throws(() => createApp(dir, { rust: true, with: ['linux'] }), /--rust: its scaffold has host crates for ios and mac; for linux, make the app, then copy apps\/sparkline's crate for it/);
    assert.equal(existsSync(dir), false);
    assert.throws(() => createApp(dir, { rust: true, snapback: {} }), /--rust and --snapback are separate starters/);
    assert.equal(existsSync(dir), false);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
});

test('a Rust app\'s source type never takes a name its starter or Rust already uses', async () => {
  const { rustNames, rustListing } = await import('./app-rust.mjs');
  const { copyableSource } = await import('../host/web-js/module.mjs');
  for (const [title, type] of [['Greeting', 'GreetingApp'], ['Value', 'ValueApp'], ['Data Source', 'DataSourceApp'], ['Self', 'SelfApp'], ['Field Log', 'FieldLog'], ['3D', 'App3D'], ['API', 'APIApp'], ['TV', 'TVApp'], ['Q1', 'Q1App']])
  {
    assert.equal(rustNames('x', title).type, type, title);
    // The web build copies the bake's source: every name the scaffold gives it must be one it copies.
    assert.ok(copyableSource(`x_data::${type}::default()`), type);
  }
  const listing = rustListing('Created /a\n  bun exact.mjs pattern      start\n  bun exact.mjs web          the web dev loop\n  bun exact.mjs linux        build\n  bun exact.mjs windows --run  on');
  assert.equal(listing, 'Created /a (its data is Rust: data/src/lib.rs, which apple/ and web/ link)\n  bun exact.mjs web          the web dev loop');
});
