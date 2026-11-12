// `exact add`'s install step: the package manager an app's tree names (scripts/package-manager.mjs).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

function inDir(body) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-add-'));
  try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('add installs with the package manager the app\'s tree names, or with none under --no-install', async () => {
  const { packageManager } = await import('./package-manager.mjs');
  inDir(dir => {
    const app = resolve(dir, 'workspace/apps/web');
    mkdirSync(app, { recursive: true });
    mkdirSync(resolve(dir, '.git'));
    assert.equal(packageManager(app), 'bun', 'none says: Bun, stopping at the repository');
    writeFileSync(resolve(dir, 'workspace/pnpm-lock.yaml'), '');
    assert.equal(packageManager(app), 'pnpm', 'the nearest lockfile');
    writeFileSync(resolve(app, 'package.json'), JSON.stringify({ packageManager: 'yarn@4.5.0' }));
    assert.equal(packageManager(app), 'yarn', 'a packageManager field');
  });
  inDir(dir => {
    // A pnpm workspace that says so at its root, with a bun.lock left over in the app.
    const app = resolve(dir, 'apps/web');
    mkdirSync(app, { recursive: true });
    mkdirSync(resolve(dir, '.git'));
    writeFileSync(resolve(dir, 'package.json'), JSON.stringify({ packageManager: 'pnpm@9.12.0' }));
    writeFileSync(resolve(app, 'package.json'), JSON.stringify({ name: 'web' }));
    writeFileSync(resolve(app, 'bun.lock'), '');
    assert.equal(packageManager(app), 'pnpm', 'a declaration wins over a nearer lockfile');
  });
  inDir(dir => {
    const root = resolve(import.meta.dir, '..'), app = resolve(dir, 'shop');
    const made = spawnSync(process.execPath, [resolve(root, 'scripts/exact.mjs'), 'new', app], { encoding: 'utf8' });
    assert.equal(made.status, 0, made.stdout + made.stderr);
    writeFileSync(resolve(dir, 'pnpm-lock.yaml'), '');
    const added = spawnSync(process.execPath, ['exact.mjs', 'add', 'convex', '--no-install'], { cwd: app, encoding: 'utf8', env: { ...process.env, EXACT: root } });
    assert.equal(added.status, 0, added.stdout + added.stderr);
    assert.match(added.stdout, /@exact\/convex is file:.*, not installed: run pnpm install\./);
    assert.ok(JSON.parse(readFileSync(resolve(app, 'package.json'), 'utf8')).dependencies['@exact/convex']);
    assert.ok(!existsSync(resolve(app, 'node_modules/@exact/convex')));
  });
});

