// share's experimental iPhone build on Exact Build (scripts/exact-build.mjs), against a stand-in for the service.
import { expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { gunzipSync } from 'node:zlib';
import { exactBuildReason, sdkSource } from './exact-build.mjs';
import { createApp } from '../game/new.mjs';

test('share builds on Exact Build only with EXACT_EXPERIMENTAL=1, and then with EXACT_BUILD=1 or no iOS build here', () => {
  const none = () => ({ status: 1 }), xcode = () => ({ status: 0 });
  const home = mkdtempSync(resolve(tmpdir(), 'exact-build-home-'));
  try {
    const on = { EXACT_EXPERIMENTAL: '1', HOME: home };
    expect(exactBuildReason({ HOME: home }, 'linux', none)).toBe(null);
    expect(exactBuildReason({ HOME: home, EXACT_BUILD: '1' }, 'linux', none)).toBe(null);
    expect(exactBuildReason(on, 'linux', none)).toMatch(/^this machine has no iOS build of its own/);
    expect(exactBuildReason({ ...on, EXACT_BUILD: '1' }, 'linux', none)).toBe('EXACT_BUILD=1');
    // A system build that is set up, even partly, is this machine's: its own build says what it lacks.
    expect(exactBuildReason({ ...on, EXACT_APPLE_LLVM_BIN: '/opt/llvm/bin' }, 'linux', none)).toBe(null);
    mkdirSync(resolve(home, '.swiftpm/swift-sdks/darwin.artifactbundle'), { recursive: true });
    expect(exactBuildReason(on, 'linux', none)).toBe(null);
    expect(exactBuildReason({ ...on, EXACT_BUILD: '1' }, 'linux', none)).toBe('EXACT_BUILD=1');
    expect(exactBuildReason(on, 'darwin', xcode)).toBe(null);
    expect(exactBuildReason({ ...on, DEVELOPER_DIR: '/Applications/Xcode-27.app/Contents/Developer' }, 'darwin', none)).toBe(null);
    if (!existsSync('/Applications/Xcode.app')) expect(exactBuildReason(on, 'darwin', none)).toMatch(/no Xcode/);
    expect(exactBuildReason(on, 'win32', xcode)).toMatch(/iOS builds need macOS or Linux/);
  } finally { rmSync(home, { recursive: true, force: true }); }
});

test.skipIf(!Bun.which('npx'))('share signs in, builds on Exact Build with its build number, and passes the IPA to AppDrop', async () => {
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-build-share-'));
  const ipa = Buffer.from('PK\x03\x04 an IPA from the service');
  const builds = [], blobs = new Set(), seen = [], auths = new Set();
  let polls = 0;
  const server = Bun.serve({
    port: 0,
    async fetch(request) {
      const url = new URL(request.url), auth = request.headers.get('authorization');
      seen.push(`${request.method} ${url.pathname}`);
      if (auth) auths.add(auth);
      if (url.pathname === '/auth/device') return Response.json({ url: 'https://expo.example/login/abc', request_id: 'req1' });
      if (url.pathname === '/auth/device/req1') {
        const { match } = await request.json();
        return match === '1234' ? Response.json({ token: 't'.repeat(40), username: 'tester' }) : Response.json({ error: 'invalid_grant' }, { status: 400 });
      }
      if (auth !== `Bearer ${'t'.repeat(40)}`) return Response.json({ error: 'A valid build token is required' }, { status: 401 });
      if (url.pathname === '/sdk') return Response.json({ upstream: url.searchParams.get('exact'), snapshot: 'test' });
      if (url.pathname === '/build') {
        const { files } = await request.json();
        builds.push({ query: Object.fromEntries(url.searchParams), files });
        const missing = [...new Set(Object.values(files).map(([id]) => id))].filter((id) => !blobs.has(id));
        builds.at(-1).missing = missing.length;
        return Response.json({ id: `b${builds.length}`, missing });
      }
      if (url.pathname === '/blobs') {
        let data = gunzipSync(Buffer.from(await request.arrayBuffer()));
        while (data.length) { const end = data.indexOf(10), [id, size] = data.subarray(0, end).toString().split(' '); blobs.add(id); data = data.subarray(end + 1 + Number(size)); }
        return Response.json({ ok: true });
      }
      if (/^\/b\d+\/$/.test(url.pathname)) return Response.json(++polls % 2 ? { status: 'building' } : { status: 'succeeded', bytes: ipa.length, repacked: false });
      if (/^\/b\d+\.ipa$/.test(url.pathname)) return new Response(ipa);
      return Response.json({ error: 'Not found' }, { status: 404 });
    },
  });
  try {
    // Only Bun, git and sh on PATH (and npx, beside Bun): no Xcode, no Rust.
    const bin = resolve(scratch, 'bin'), app = resolve(scratch, 'shared');
    mkdirSync(bin);
    symlinkSync(process.execPath, resolve(bin, 'bun'));
    symlinkSync(Bun.which('git'), resolve(bin, 'git'));
    symlinkSync('/bin/sh', resolve(bin, 'sh'));
    // AppDrop's CLI, as a package npx runs from this directory (EXACT_APPDROP_CLI): it records the IPA's size.
    const appdrop = resolve(scratch, 'appdrop');
    mkdirSync(appdrop);
    writeFileSync(resolve(appdrop, 'package.json'), JSON.stringify({ name: 'appdrop-stand-in', version: '1.0.0', bin: { 'appdrop-stand-in': 'cli.js' } }));
    writeFileSync(resolve(appdrop, 'cli.js'), `#!/usr/bin/env node\nconst fs = require('fs');\nfs.writeFileSync(${JSON.stringify(resolve(scratch, 'uploaded'))}, String(fs.statSync(process.argv[2]).size));\nconsole.log(JSON.stringify({ url: 'https://appdrop.app/d/abc123', revision: 1 }));\n`, { mode: 0o755 });
    createApp(app);
    const manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8'));
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ ...manifest, deploy: { ...manifest.deploy, store: { ...manifest.deploy?.store, ios: '0' } } }, null, 2));
    // EXACT_VERSION, for an app.json that names no version: the local build's version, so the remote build's too.
    const env = { HOME: scratch, PATH: bin, EXACT_FEEDBACK: '0', EXACT_EXPERIMENTAL: '1', EXACT_BUILD: '1', EXACT_BUILD_URL: server.url.origin, EXACT_APPDROP_CLI: appdrop, EXACT_VERSION: '9.7.1' };
    // Exact Build's own token, which another service (EXACT_BUILD_URL) must never be sent.
    const official = 'o'.repeat(40);
    writeFileSync(resolve(scratch, '.exact-build-token'), official, { mode: 0o600 });
    const own = (prefix) => readdirSync(scratch).filter((name) => name.startsWith(prefix) && name !== '.exact-build-token' && name !== '.exact-build-login.json');
    // Not spawnSync: the stand-in serves from this process's event loop.
    const share = async (...args) => {
      const extra = typeof args[0] === 'object' ? args.shift() : {};
      const child = Bun.spawn([process.execPath, resolve(app, 'exact.mjs'), 'share', '--json', ...args], { cwd: app, env: { ...env, ...extra }, stdout: 'pipe', stderr: 'pipe' });
      const [stdout, stderr, status] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
      return { stdout, stderr, status };
    };

    const first = await share();
    expect(first.status).toBe(1);
    expect(JSON.parse(first.stdout)).toMatchObject({ code: 'exact_build_login', url: 'https://expo.example/login/abc' });
    expect(first.stderr).toContain('share: EXACT_BUILD=1, so the iPhone build runs on Exact Build.');
    expect(own('.exact-build-login-')).toHaveLength(1);
    expect(statSync(resolve(scratch, own('.exact-build-login-')[0])).mode & 0o777).toBe(0o600);

    // --login is experimental too: without EXACT_EXPERIMENTAL=1 it signs in to nothing.
    const unflagged = await share({ EXACT_EXPERIMENTAL: '' }, '--login', '1234');
    // Through the app CLI, this machine's preflight refuses first; share.mjs itself, as a machine whose preflight
    // passes would reach it, refuses the login by name.
    expect(unflagged.status).not.toBe(0);
    const direct = Bun.spawn([process.execPath, resolve(import.meta.dir, 'share.mjs'), '--app', app, '--json', '--login', '1234'], { cwd: app, env: { ...env, EXACT_EXPERIMENTAL: '' }, stdout: 'pipe', stderr: 'pipe' });
    const [said, status] = await Promise.all([new Response(direct.stdout).text(), direct.exited]);
    expect(status).toBe(1);
    expect(JSON.parse(said)).toMatchObject({ code: 'exact_build_login', error: expect.stringContaining('EXACT_EXPERIMENTAL=1') });
    expect(seen).not.toContain('POST /auth/device/req1');
    expect(own('.exact-build-token-')).toHaveLength(0);

    const shared = await share('--login', '1234');
    expect(shared.status, shared.stderr).toBe(0);
    expect(JSON.parse(shared.stdout)).toMatchObject({ url: 'https://appdrop.app/d/abc123', build: 1, action: 'created' });
    expect(own('.exact-build-token-')).toHaveLength(1);
    expect(statSync(resolve(scratch, own('.exact-build-token-')[0])).mode & 0o777).toBe(0o600);
    expect(readFileSync(resolve(scratch, '.exact-build-token'), 'utf8')).toBe(official);
    expect(own('.exact-build-login-')).toHaveLength(0);
    expect(builds[0].query).toMatchObject({ profile: 'release', buildNumber: '1', version: '9.7.1' });
    // deploy.store.ios is "0": no update stream, so no genesis.
    expect(builds[0].query.updateGenesis).toBeUndefined();
    expect(builds[0].query.base).toBeUndefined();
    expect(Object.keys(builds[0].files)).toContain('shared/app.contract');
    expect(Object.keys(builds[0].files).some((path) => /^shared\/(\.exact|node_modules)\//.test(path))).toBe(false);
    expect(readFileSync(resolve(scratch, 'uploaded'), 'utf8').trim()).toBe(String(ipa.length));
    expect(JSON.parse(readFileSync(resolve(app, '.exact/exact-build.json'), 'utf8'))).toEqual({ url: server.url.origin, id: 'b1' });

    // The next build, a release one, names no base and sends only the blob the service lacks, the changed app.contract.
    // (--new-link: AppDrop itself is not stood in for, and an update asks it for the link's last build.)
    writeFileSync(resolve(app, 'app.contract'), `${readFileSync(resolve(app, 'app.contract'), 'utf8')}\n`);
    const before = seen.filter((line) => line === 'POST /blobs').length;
    const again = await share('--new-link');
    expect(again.status, again.stderr).toBe(0);
    expect(builds[1].query).toMatchObject({ profile: 'release', buildNumber: String(JSON.parse(again.stdout).build) });
    expect(builds[1].query.base).toBeUndefined();
    expect(seen.filter((line) => line === 'POST /blobs').length).toBe(before + 1);
    expect(builds[1].missing).toBe(1);
    expect(auths.has(`Bearer ${official}`)).toBe(false);
  } finally {
    server.stop(true);
    rmSync(scratch, { recursive: true, force: true });
  }
}, 240_000);

// A checkout's changes go as a patch whether it is a clone or a linked worktree (whose `.git` is a file naming its Git
// directory), and a Git step that fails says so instead of sending a wrong patch.
test('the SDK patch is made in a linked worktree, and a failed Git step refuses', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-build-worktree-'));
  const git = (cwd, ...args) => { const r = spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', '-C', cwd, ...args], { encoding: 'utf8' }); if (r.status !== 0) throw new Error(r.stderr); return r.stdout.trim(); };
  try {
    const main = resolve(dir, 'main'), linked = resolve(dir, 'linked');
    mkdirSync(main);
    git(main, 'init', '-q');
    writeFileSync(resolve(main, 'sdk.txt'), 'one\n');
    git(main, 'add', '.'); git(main, 'commit', '-qm', 'one');
    const head = git(main, 'rev-parse', 'HEAD');
    git(main, 'worktree', 'add', '-q', linked);
    expect(statSync(resolve(linked, '.git')).isFile()).toBe(true);
    writeFileSync(resolve(linked, 'sdk.txt'), 'two\n');
    const { commit, patch } = await sdkSource(linked, resolve(dir, 'app'), async () => ({ upstream: head }));
    expect(commit).toBe(head);
    expect(patch.toString()).toContain('+two');
    // A snapshot commit this checkout lacks and cannot fetch (no remote) refuses by name.
    await expect(sdkSource(linked, resolve(dir, 'app'), async () => ({ upstream: 'f'.repeat(40) }))).rejects.toThrow(/git fetch of the SDK snapshot's commit/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
