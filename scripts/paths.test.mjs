import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { posix, win32 } from 'node:path';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, parse, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { pathFrom, pathWithin, slashRelative, systemChrome, within } from './paths.mjs';
import { chromium } from './agent-launch.mjs';
import * as paths from './paths.mjs';

test('a path is within a directory by its own platform separator', () => {
  assert.ok(within('/w/app', '/w/app', posix));
  assert.ok(within('/w/app', '/w/app/src/lib.rs', posix));
  assert.ok(within('/', '/w', posix));
  assert.ok(!within('/w/app', '/w/apps/x', posix));
  assert.ok(!within('/w/app', '/w', posix));
  assert.ok(within('C:\\w\\app', 'C:\\w\\app\\src\\lib.rs', win32));
  assert.ok(within('C:\\w\\app', 'C:/w/app/src/lib.rs', win32));
  assert.ok(within('C:\\w\\app', 'c:\\W\\App\\src', win32));
  assert.ok(within('C:\\', 'C:\\w', win32));
  assert.ok(!within('C:\\w\\app', 'C:\\w\\apps\\x', win32));
  assert.ok(!within('C:\\w\\app', 'C:\\w', win32));
  assert.ok(!within('C:\\w\\app', 'D:\\w\\app\\x', win32));
});

test('a relative path is written with slashes on either platform', () => {
  assert.equal(slashRelative('/w/app', '/w/app/assets/a.png', posix), 'assets/a.png');
  assert.equal(slashRelative('C:\\w\\app', 'C:\\w\\app\\assets\\a.png', win32), 'assets/a.png');
  assert.equal(slashRelative('C:\\w\\app', 'C:\\w\\other', win32), '../other');
});

test('a written path from the filesystem root has one leading separator', () => {
  const cwd = parse(process.cwd()).root;
  const script = `import { writtenPath } from ${JSON.stringify(import.meta.resolve('./paths.mjs'))}; console.log(writtenPath('tools'));`;
  const result = spawnSync(process.execPath, ['-e', script], { cwd, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), resolve(cwd, 'tools'));
});

test('the process command recovers the invoked checkout entry on each platform', () => {
  const root = resolve(import.meta.dir, '..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const parent = realpathSync(mkdtempSync(resolve(root, "target/path reference's-")));
  try {
    const app = resolve(parent, 'repo/mobile'), sdk = resolve(parent, 'sdk');
    mkdirSync(app, { recursive: true }); mkdirSync(sdk);
    symlinkSync(sdk, resolve(parent, 'repo/checkout'), 'dir');
    const file = resolve(sdk, 'entry.mjs'), entry = resolve(parent, 'repo/checkout/entry.mjs');
    writeFileSync(file, '');
    const argv = ['bun', file, 'new', 'an app'];
    for (const [platform, command] of [
      ['darwin', `bun ${entry} new an app\n`],
      ['linux', ['bun', entry, 'new', 'an app', ''].join('\0')],
      ['win32', `"bun" "${entry}" new "an app"\r\n`],
    ]) assert.equal(paths.commandPath(file, command, argv, platform), entry);
    assert.equal(paths.commandPath(file, 'bun /missing/entry.mjs new an app\n', argv, 'darwin'), file);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test.each([false, true])('committed references use the local link while build paths are physical (linked: %s)', linkedTree => {
  const parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-worktree-reference-')));
  const main = resolve(parent, 'repo'), linked = linkedTree ? resolve(parent, 'nested/trees/topic') : main;
  const git = (cwd, ...args) => {
    const result = spawnSync('git', ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', ...args], { cwd, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  };
  try {
    mkdirSync(main); git(main, 'init', '-q'); git(main, 'commit', '--allow-empty', '-qm', 'Create the fixture');
    if (linkedTree) git(main, 'worktree', 'add', '-q', '--detach', linked);
    const app = resolve(linked, 'mobile');
    mkdirSync(resolve(app, '.exact'), { recursive: true });
    writeFileSync(resolve(app, 'app.json'), '{}');
    // Both Astra's sibling checkout and an external checkout have the same committed reference.
    for (const sdk of [resolve(linked, 'sdk'), resolve(parent, 'external')]) {
      mkdirSync(resolve(sdk, 'packages/probe'), { recursive: true });
      symlinkSync(sdk, resolve(app, '.exact/checkout'), process.platform === 'win32' ? 'junction' : 'dir');
      for (const [from, expected] of [[app, '.exact/checkout'], [resolve(app, 'data'), '../.exact/checkout']]) {
        const value = pathFrom(from, resolve(sdk, 'packages/probe'));
        assert.equal(value, expected + '/packages/probe');
        assert.equal(realpathSync(resolve(from, value)), resolve(sdk, 'packages/probe'));
      }
      const shell = resolve(app, '.exact/shells');
      assert.equal(pathWithin(shell, resolve(app, '.exact/checkout/packages/probe')), slashRelative(shell, resolve(sdk, 'packages/probe')));
      rmSync(resolve(app, '.exact/checkout'));
    }
    const local = resolve(linked, 'packages/local');
    mkdirSync(local, { recursive: true });
    assert.equal(pathFrom(app, local), '../packages/local');
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

// LLP 1156.006.000 D9: a Chrome installed here is named as the CHROME to set beside a missing pinned browser, and
// never launched unasked.
test('a missing pinned browser names the Chrome installed here', () => {
  const full = '/absent/ms-playwright/chromium-1248/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing';
  const at = (system) => chromium({ environment: {}, platform: 'darwin', arch: 'arm64', full, system });
  assert.match(at(() => undefined).unavailable, /headless shell is missing .*install chromium-headless-shell, or set CHROME to a browser$/);
  const here = at(() => '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome');
  assert.ok(here.unavailable.endsWith("install chromium-headless-shell, or set CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' to use the Chrome installed here (not the pinned revision)"), here.unavailable);
  assert.match(here.executable, /chrome-headless-shell$/);
  const onPath = '/opt/tools/bin/chromium';
  assert.equal(systemChrome({ environment: { PATH: dirname(onPath) }, platform: 'linux', executable: (path) => path === onPath }), onPath);
  assert.equal(systemChrome({ environment: { PATH: dirname(onPath) }, platform: 'linux', executable: () => false }), undefined);
  assert.equal(systemChrome({ environment: {}, platform: 'darwin', executable: (path) => path.endsWith('/Chromium') }), '/Applications/Chromium.app/Contents/MacOS/Chromium');
  assert.equal(systemChrome({ environment: { PATH: resolve('/x') }, platform: 'win32', executable: () => true }), undefined);
});
