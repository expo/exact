import { test, expect, spyOn } from 'bun:test';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { hostname, tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { need, notApplicable } from './coverage.mjs';
import { chromium, driveStore, exclusiveWebStore, launchFacts, removeDriveStore, webNeeds, quitBrowser, pollUntil } from './agent-launch.mjs';
import { resolveApp } from './app.mjs';
import { open, webFrame } from './agent.mjs';
import { crop, decodePng } from './png.mjs';
import { glueDeclarations } from '../host/web/tests/glue-fixture.mjs';

need('chrome', chromium().unavailable)('scaled avatars keep identical pixels after loading and remounting', async () => {
  const style = readFileSync(new URL('../host/web/index.html', import.meta.url), 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1];
  const events = glueDeclarations(readFileSync(new URL('../host/web/wasm/views.js', import.meta.url), 'utf8'), ['imageEvents']);
  let requested = Promise.withResolvers(), release = Promise.withResolvers();
  const server = Bun.serve({hostname:'127.0.0.1', port:0, async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/avatar.jpg') {
      requested.resolve(); await release.promise;
      return new Response(readFileSync(new URL('../apps/photos-fixture/assets/photos-fixture/agent-1.jpg', import.meta.url)));
    }
    if (path !== '/') return new Response(null, {status:404});
    return new Response(`<!doctype html><style>${style}</style><div id="exact-root" data-boot-ms="0" style="width:420px;height:900px;background-color:light-dark(white,#121212)"></div>
      <div style="position:absolute;left:800px;top:1600px;width:100px;height:100px"></div>
      <script>
        const imageProbes = new Map(), inflight = new Set(), IMAGE_ERROR = 'image', CORS_ERROR = 'cors';
        ${events}
        const root = document.getElementById('exact-root');
        globalThis.mount = scheme => {
          root.innerHTML = [42,52].map((size,k) => '<div style="position:absolute;left:32px;top:'+(248.4+84*k)+'px;width:'+size+'px;height:'+size+'px;display:flex;flex-direction:column;overflow:hidden;border-radius:999px;background-color:light-dark(#f2f2f7,#1c1c1e)"><img alt="" src="/avatar.jpg?'+scheme+'" style="width:'+size+'px;height:'+size+'px;object-fit:cover"><div style="position:absolute;inset:0;border:1px solid light-dark(#dce2ea99,#485b7599);border-radius:999px"></div></div>').join('');
          for (const image of document.images) imageEvents(image, () => {});
        };
        globalThis.exact = {ready:Promise.resolve(), agentSettled:async () => ({})};
      </script>`, {headers:{'content-type':'text/html'}});
  }});
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-avatar-capture-'));
  let session;
  try {
    session = await open({host:'web', url:server.url.href, size:[420,900]});
    const {evaluate, call} = session.carrier;
    const dpr = await evaluate(() => devicePixelRatio);
    const pixels = [420,900].map(size => Math.round(size * dpr));
    let viewport;
    for (const scheme of ['light','dark']) {
      await session.prefer({'prefers-color-scheme':scheme});
      requested = Promise.withResolvers(); release = Promise.withResolvers();
      await evaluate(`mount('${scheme}')`);
      await requested.promise;
      // Paint the background while the JPEG is still loading, then compare its settled picture with a cached mount.
      await webFrame(evaluate);
      await call('Page.captureScreenshot', {format:'png'});
      release.resolve();
      let baseline, geometry;
      for (let mount = 0; mount < 3; mount++) {
        if (mount) await evaluate(`mount('${scheme}')`);
        const facts = await evaluate(async () => Promise.all([...document.images].map(async image => {
          await image.decode();
          return {complete:image.complete, natural:[image.naturalWidth,image.naturalHeight], rect:image.getBoundingClientRect().toJSON()};
        })));
        expect(facts.map(image => [image.complete, image.natural])).toEqual([[true,[480,360]],[true,[480,360]]]);
        geometry ??= facts.map(image => image.rect);
        expect(facts.map(image => image.rect)).toEqual(geometry);
        for (let repeat = 0; repeat < 2; repeat++) {
          const path = resolve(dir, `${scheme}-${mount}-${repeat}.png`);
          await session.screenshot(path);
          const picture = decodePng(readFileSync(path));
          viewport = picture;
          expect([picture.width,picture.height]).toEqual(pixels);
          baseline ??= picture.data;
          expect(Buffer.compare(baseline, picture.data)).toBe(0);
        }
        expect(await evaluate(() => [...document.images].map(image => image.getBoundingClientRect().toJSON()))).toEqual(geometry);
      }
    }
    await evaluate(() => scrollTo(40, 100));
    const scroll = await evaluate(() => [scrollX, scrollY]);
    expect(scroll).toEqual([40,100]);
    const path = resolve(dir, 'scrolled.png');
    await session.screenshot(path);
    const picture = decodePng(readFileSync(path));
    expect([picture.width,picture.height]).toEqual(pixels);
    const [x,y] = [40,100].map(offset => Math.round(offset * dpr));
    const width = pixels[0] - x, height = pixels[1] - y;
    expect(Buffer.compare(crop(picture, 0, 0, width, height).data, crop(viewport, x, y, width, height).data)).toBe(0);
    expect(await evaluate(() => [scrollX, scrollY])).toEqual(scroll);
  } finally {
    release.resolve();
    try { await session?.close(); }
    finally { server.stop(true); rmSync(dir, {recursive:true, force:true}); }
  }
}, 30000);

const posix = process.platform === 'win32' ? 'SingletonLock is a POSIX symlink' : null;
for (const source of ['SingletonLock', 'browser-owner']) {
  notApplicable(source === 'SingletonLock' ? posix : null)(
    `a named profile only admits an absent ${source} or a verifiably dead local owner`, async () => {
      const cache = mkdtempSync(resolve(tmpdir(), 'exact-profile-owner-'));
      const options = {app: 'caltrain', browser: 'chrome', storage: 'proof', env: {XDG_CACHE_HOME: cache, HOME: cache}};
      const {base, profile} = driveStore(resolveApp(options.app), options.storage, options.env);
      const lock = source === 'SingletonLock' ? resolve(profile, source) : resolve(base + '-locks', options.storage, source);
      const marker = resolve(profile, 'kept');
      const put = value => source === 'SingletonLock' ? symlinkSync(value, lock) : writeFileSync(lock, value);
      const read = () => source === 'SingletonLock' ? readlinkSync(lock) : readFileSync(lock, 'utf8');
      mkdirSync(profile, {recursive: true}); writeFileSync(marker, 'keep');
      let launches = 0;
      const launch = async () => { launches++; return {close: async () => {}}; };
      const attempt = async () => {
        try { const carrier = await exclusiveWebStore(options, launch); await carrier.close(); return null; }
        catch (error) { return error.message; }
      };
      try {
        expect(await attempt()).toBe(null);
        const dead = spawn(process.execPath, ['-e', '']);
        await once(dead, 'exit');
        put(`${hostname()}-${dead.pid}`);
        expect(await attempt()).toBe(null);
        if (source === 'SingletonLock') {
          expect(read()).toBe(`${hostname()}-${dead.pid}`);
          rmSync(lock);
        } else expect(existsSync(lock)).toBe(false);
        for (const value of [
          `${hostname()}-${process.pid}`, `other-${hostname()}-${dead.pid}`, 'malformed',
          `${hostname()}-0`, `${hostname()}--1`, `${hostname()}-1x`, `${hostname()}-2147483648`,
          `${hostname()}-999999999999999999999`, `${hostname()}-`, `${hostname()}-1\n`,
        ]) {
          put(value);
          const refusal = await attempt();
          expect(refusal).toContain(profile);
          expect(refusal).toContain(source);
          expect(refusal).toContain('another store');
          expect(read()).toBe(value);
          expect(readFileSync(marker, 'utf8')).toBe('keep');
          rmSync(lock);
        }
        put(`${hostname()}-${dead.pid}`);
        const kill = process.kill.bind(process);
        const probe = spyOn(process, 'kill').mockImplementation((pid, signal) => {
          if (pid === dead.pid && signal === 0) throw Object.assign(new Error('not permitted'), {code: 'EPERM'});
          return kill(pid, signal);
        });
        try {
          expect(await attempt()).toContain(`cannot verify Chrome ${dead.pid}: EPERM`);
          expect(read()).toBe(`${hostname()}-${dead.pid}`);
        } finally { probe.mockRestore(); rmSync(lock); }
        if (source === 'SingletonLock') writeFileSync(lock, 'not a symlink');
        else mkdirSync(lock);
        expect(await attempt()).toContain(`cannot read ${source}`);
        expect(launches).toBe(2);
      } finally { rmSync(cache, {recursive: true, force: true}); }
    },
  );
}

test('a failed named launch retains the browser owner only if a browser started', async () => {
  const cache = mkdtempSync(resolve(tmpdir(), 'exact-profile-launch-'));
  const options = {app: 'shelf', browser: 'chrome', storage: 'proof', env: {XDG_CACHE_HOME: cache, HOME: cache}};
  const {base, profile} = driveStore(resolveApp(options.app), options.storage, options.env);
  const owner = resolve(base + '-locks', options.storage, 'browser-owner');
  try {
    await expect(exclusiveWebStore(options, async () => { throw new Error('before spawn'); })).rejects.toThrow('before spawn');
    expect(existsSync(owner)).toBe(false);
    await expect(exclusiveWebStore(options, async launched => {
      launched.beforeSpawn();
      expect(readFileSync(owner, 'utf8')).toBe('');
      throw new Error('spawn failed');
    })).rejects.toThrow('spawn failed');
    expect(existsSync(owner)).toBe(false);
    await expect(exclusiveWebStore(options, async launched => {
      launched.beforeSpawn();
      launched.onProcess({pid: process.pid});
      throw new Error('after spawn');
    })).rejects.toThrow('after spawn');
    expect(readFileSync(owner, 'utf8')).toBe(`${hostname()}-${process.pid}`);
    await expect(exclusiveWebStore(options, () => { throw new Error('must refuse first'); })).rejects.toThrow(`Chrome ${process.pid}`);
    expect(readFileSync(owner, 'utf8')).toBe(`${hostname()}-${process.pid}`);
    expect(existsSync(profile)).toBe(false);
  } finally { rmSync(cache, {recursive: true, force: true}); }
});

// A driver killed after `beforeSpawn` and before `onProcess` leaves an owner record without a PID.
test('a browser-owner without a PID defers to the SingletonLock', async () => {
  const cache = mkdtempSync(resolve(tmpdir(), 'exact-profile-unrecorded-'));
  const options = {app: 'shelf', browser: 'chrome', storage: 'proof', env: {XDG_CACHE_HOME: cache, HOME: cache}};
  const {base, profile} = driveStore(resolveApp(options.app), options.storage, options.env);
  const owner = resolve(base + '-locks', options.storage, 'browser-owner'), singleton = resolve(profile, 'SingletonLock');
  const launch = async () => ({close: async () => {}});
  try {
    mkdirSync(dirname(owner), {recursive: true}); writeFileSync(owner, '');
    if (!posix) {
      mkdirSync(profile, {recursive: true}); symlinkSync(`${hostname()}-${process.pid}`, singleton);
      await expect(exclusiveWebStore(options, launch)).rejects.toThrow(`Chrome ${process.pid} still owns this profile (${singleton})`);
      expect(readFileSync(owner, 'utf8')).toBe('');
      rmSync(singleton);
    }
    await (await exclusiveWebStore(options, launch)).close();
    expect(existsSync(owner)).toBe(false);
  } finally { rmSync(cache, {recursive: true, force: true}); }
});

test('a named browser that does not exit on close keeps its owner record and releases the drive lock', async () => {
  const cache = mkdtempSync(resolve(tmpdir(), 'exact-profile-close-'));
  const options = {app: 'shelf', browser: 'chrome', storage: 'proof', env: {XDG_CACHE_HOME: cache, HOME: cache}};
  const {base} = driveStore(resolveApp(options.app), options.storage, options.env);
  const owner = resolve(base + '-locks', options.storage, 'browser-owner');
  try {
    const carrier = await exclusiveWebStore(options, async launched => {
      launched.beforeSpawn(); launched.onProcess({pid: process.pid});
      return {close: async () => { throw new Error(`Chrome ${process.pid} did not exit`); }};
    });
    await expect(carrier.close()).rejects.toThrow('did not exit');
    expect(readFileSync(owner, 'utf8')).toBe(`${hostname()}-${process.pid}`);
    await expect(exclusiveWebStore(options, () => { throw new Error('must refuse first'); })).rejects.toThrow(`Chrome ${process.pid} still owns this profile`);
    // A throwaway store's cleanup keeps the profile and the record while that browser may live.
    const store = driveStore(resolveApp(options.app), options.storage, options.env);
    mkdirSync(store.profile, {recursive: true});
    expect(await removeDriveStore(store, options.storage)).toBe(false);
    expect([existsSync(store.profile), existsSync(owner)]).toEqual([true, true]);
    const closed = await exclusiveWebStore({...options, storage: 'closed'}, async launched => {
      launched.beforeSpawn(); launched.onProcess({pid: process.pid});
      return {close: async () => {}};
    });
    await closed.close();
    const gone = driveStore(resolveApp(options.app), 'closed', options.env);
    mkdirSync(gone.profile, {recursive: true});
    expect(await removeDriveStore(gone, 'closed')).toBe(true);
    expect([existsSync(gone.profile), existsSync(resolve(base + '-locks', 'closed'))]).toEqual([false, false]);
  } finally { rmSync(cache, {recursive: true, force: true}); }
});

// The backend preflight precedes any reset of the page (a Snapback app's refusal must leave its origin alone).
test('a compatible Chrome reuse asks the backend before it resets the page', async () => {
  const dist = mkdtempSync(resolve(tmpdir(), 'exact-reuse-preflight-')), events = [];
  mkdirSync(resolve(dist, '.gen'));
  writeFileSync(resolve(dist, '.gen/admission-data.js'), `export const tsGrantSet=createGrantSet(${JSON.stringify({entries: [[0, 'net.fetch http://127.0.0.1:1', [], null]]})});`);
  const reuse = {browser: 'chrome', launchFacts: launchFacts({}), reset: async () => { events.push('reset'); }, close: async () => { events.push('close'); }};
  const said = spyOn(console, 'error').mockImplementation(line => { events.push(String(line)); });
  try {
    const {open} = await import('./agent.mjs');
    expect((await open({app: 'shelf', webDist: dist, reuse})).carrier).toBe(reuse);
    expect(events).toEqual([expect.stringContaining('no backend at http://127.0.0.1:1'), 'reset']);
  } finally { said.mockRestore(); rmSync(dist, {recursive: true, force: true}); }
});

test('a named drive interrupted before browser spawn leaves its profile reusable', async () => {
  const cache = mkdtempSync(resolve(tmpdir(), 'exact-profile-preflight-'));
  const options = {app: 'shelf', browser: 'chrome', storage: 'proof', url: 'http://127.0.0.1:9/', env: {XDG_CACHE_HOME: cache, HOME: cache}};
  const {base, profile} = driveStore(resolveApp(options.app), options.storage, options.env);
  const owner = resolve(base + '-locks', options.storage, 'browser-owner'), ready = resolve(cache, 'ready');
  mkdirSync(profile, {recursive: true});
  const marker = resolve(profile, 'kept'); writeFileSync(marker, 'keep');
  const source = `import {mock} from 'bun:test';
import {writeFileSync} from 'node:fs';
const http = await import('node:http'), createServer = http.createServer;
mock.module('node:http', () => ({...http, createServer: (...args) => {
  const server = createServer(...args);
  server.listen = () => { writeFileSync(${JSON.stringify(ready)}, 'listening'); return server; };
  return server;
}}));
const {open} = await import(${JSON.stringify(resolve(import.meta.dir, 'agent.mjs'))});
await open(${JSON.stringify(options)});`;
  const driver = spawn(process.execPath, ['-e', source], {stdio: ['ignore', 'ignore', 'pipe']});
  const exited = once(driver, 'exit');
  let stderr = '';
  driver.stderr.on('data', chunk => { stderr += chunk; });
  try {
    expect(await pollUntil(() => existsSync(ready) ? 'ready' : driver.exitCode !== null ? stderr : null, Boolean, {bound: 10000})).toBe('ready');
    driver.kill('SIGKILL'); await exited;
    expect(stderr).toBe('');
    expect(existsSync(owner)).toBe(false);
    if (!posix) symlinkSync(`${hostname()}-${driver.pid}`, resolve(profile, 'SingletonLock'));
    const attempt = async () => {
      try {
        const carrier = await exclusiveWebStore(options, async () => ({close: async () => {}}));
        await carrier.close(); return 'released';
      } catch (error) { return error.message; }
    };
    expect(await pollUntil(attempt, value => !value.includes('profile lock failed'), {bound: 10000})).toBe('released');
    expect(readFileSync(marker, 'utf8')).toBe('keep');
  } finally {
    if (driver.exitCode === null && driver.signalCode === null) driver.kill('SIGKILL');
    await exited;
    rmSync(cache, {recursive: true, force: true});
  }
}, 15000);

for (const [build, needs] of [['default', webNeeds(resolveApp('shelf'))], ['full', ['perf']]]) {
  const chrome = chromium({needs});
  need('chrome', chrome.unavailable)(
    `a surviving ${build} browser refuses a named drive after its driver dies, then releases the profile`,
    async () => {
      // Keep the browser as our child so the test can reap it after killing the separate driver.
      const cache = mkdtempSync(resolve(tmpdir(), 'exact-profile-lifetime-'));
      const options = {app: 'shelf', browser: 'chrome', storage: 'proof', env: {XDG_CACHE_HOME: cache, HOME: cache}};
      const {base, profile} = driveStore(resolveApp(options.app), options.storage, options.env);
      const ready = resolve(cache, 'ready');
      const owner = resolve(base + '-locks', options.storage, 'browser-owner');
      const source = `import {exclusiveWebStore} from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
import {mkdirSync, writeFileSync} from 'node:fs';
import {once} from 'node:events';
await exclusiveWebStore(${JSON.stringify(options)}, async launched => {
  mkdirSync(${JSON.stringify(profile)}, {recursive: true});
  writeFileSync(${JSON.stringify(ready)}, 'ready');
  const [pid] = await once(process.stdin, 'data');
  launched.beforeSpawn();
  launched.onProcess({pid: Number(String(pid))});
  writeFileSync(${JSON.stringify(ready)}, 'owned');
  return {close: async () => {}};
});`;
      let driver, driverExit, browser, browserExit, session, stderr = '';
      const server = Bun.serve({hostname: '127.0.0.1', port: 0, fetch: () => new Response(`
<div id="exact-root" data-boot-ms="1"></div><script>globalThis.exact = {ready: Promise.resolve()};</script>`,
        {headers: {'content-type': 'text/html'}})});
      try {
        driver = spawn(process.execPath, ['-e', source], {stdio: ['pipe', 'ignore', 'pipe']});
        driverExit = once(driver, 'exit');
        driver.stderr.on('data', chunk => { stderr += chunk; });
        expect(await pollUntil(() => existsSync(ready), Boolean, {bound: 10000})).toBe(true);
        browser = spawn(chrome.executable, [
          '--headless=new', '--no-sandbox', '--disable-gpu', '--disable-background-networking',
          '--disable-component-update', '--disable-sync', '--no-first-run', '--no-default-browser-check',
          '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
        ], {stdio: 'ignore'});
        browserExit = once(browser, 'exit');
        expect(await pollUntil(() => existsSync(resolve(profile, 'DevToolsActivePort')), Boolean, {bound: 10000})).toBe(true);
        driver.stdin.write(String(browser.pid));
        expect(await pollUntil(() => readFileSync(ready, 'utf8'), value => value === 'owned', {bound: 10000})).toBe('owned');
        const marker = resolve(profile, 'kept'); writeFileSync(marker, 'keep');
        driver.kill('SIGKILL'); await driverExit;
        expect(stderr).toBe('');
        // EOF releases the driver's helper lock asynchronously. Wait for that refusal to end.
        const {open} = await import('./agent.mjs');
        for (const fresh of [true, false]) {
          const attempt = async () => {
            try {
              session = await open({...options, url: server.url.href, env: {...options.env, EXACT_AGENT_STORAGE_FRESH: fresh ? '1' : '0'}});
              await session.close(); session = null;
              return 'launched';
            } catch (error) { return error.message; }
          };
          const refusal = await pollUntil(attempt, value => !value.includes('profile lock failed'), {bound: 10000});
          expect(refusal).toContain(`Chrome ${browser.pid}`);
          expect(refusal).toContain(profile);
          expect(refusal).toContain('close');
          expect(readFileSync(marker, 'utf8')).toBe('keep');
          expect(() => process.kill(browser.pid, 0)).not.toThrow();
        }
        browser.kill('SIGTERM'); await browserExit;
        // Exercise the real launch path and its original onProcess callback after the owner exits.
        let child;
        session = await open({...options, url: server.url.href, onProcess: launched => { child = launched; }});
        expect(readFileSync(owner, 'utf8')).toBe(`${hostname()}-${child.pid}`);
        expect(await session.carrier.evaluate('6 * 7')).toBe(42);
        const previous = session.carrier, previousChild = child;
        session = await open({...options, url: server.url.href, reuse: previous});
        expect(session.carrier).toBe(previous);
        expect(readFileSync(owner, 'utf8')).toBe(`${hostname()}-${child.pid}`);
        session = await open({...options, url: server.url.href, reuse: previous, locale: 'fr-FR', onProcess: launched => { child = launched; }});
        expect(session.carrier).not.toBe(previous);
        expect(() => process.kill(previousChild.pid, 0)).toThrow();
        expect(readFileSync(owner, 'utf8')).toBe(`${hostname()}-${child.pid}`);
        await expect(open({...options, url: server.url.href})).rejects.toThrow('profile lock failed');
        expect(session.carrier.launchFacts.locale).toBe('fr-FR');
        expect(await session.carrier.evaluate('6 * 7')).toBe(42);
        await session.close(); session = null;
        expect(existsSync(owner)).toBe(false);
        expect(() => process.kill(child.pid, 0)).toThrow();
      } finally {
        await session?.close();
        if (driver?.exitCode === null && driver.signalCode === null) driver.kill('SIGKILL');
        if (browser?.exitCode === null && browser.signalCode === null) browser.kill('SIGKILL');
        await Promise.all([driverExit, browserExit]);
        server.stop(true);
        rmSync(cache, {recursive: true, force: true});
      }
    }, 30000,
  );
}

test('a drive asks its Chrome to quit and kills its group only after the bound when it hangs', async () => {
  const fixture = ({ exitsAfter, replies }) => {
    const child = { pid: 1, exitCode: null, signalCode: null };
    const said = [], start = performance.now(), at = () => Math.round(performance.now() - start);
    let exit;
    const exited = new Promise((resolve) => { exit = () => { child.exitCode = 0; said.push(['exit', at()]); resolve(); }; });
    const cdp = { send: (method) => {
      said.push([method, at()]);
      if (exitsAfter != null) setTimeout(exit, exitsAfter);
      return replies ? Promise.resolve({}) : exitsAfter != null ? Promise.reject(new Error('Chrome exited')) : new Promise(() => {});
    } };
    return { child, said, exited, cdp, kill: () => { said.push(['kill', at()]); if (exitsAfter == null) exit(); } };
  };
  const quit = async (f, ms) => { await quitBrowser(f.child, f.cdp, f.exited, { ms, kill: f.kill }); return f.said.map(([what]) => what); };
  // Exits on the ask, answering or closing the pipe first: well inside the bound, without a kill.
  for (const replies of [true, false]) {
    const f = fixture({ exitsAfter: 30, replies });
    expect(await quit(f, 2000)).toEqual(['Browser.close', 'exit']);
    expect(f.said.at(-1)[1]).toBeLessThan(500);
  }
  // Hung: no answer, no exit. Killed at the bound.
  const hung = fixture({});
  expect(await quit(hung, 200)).toEqual(['Browser.close', 'kill', 'exit']);
  expect(hung.said.at(-1)[1]).toBeGreaterThanOrEqual(195);
});

test('a crashed Chrome has its group cleaned up and failed termination retains the profile', async () => {
  const killed = [], cdp = { send: () => Promise.resolve({}) };
  for (const status of [{ exitCode: 1, signalCode: null }, { exitCode: null, signalCode: 'SIGSEGV' }]) {
    await quitBrowser({ pid: 1, ...status }, cdp, Promise.resolve(), { kill: () => killed.push('group') });
  }
  expect(killed).toEqual(['group', 'group']);
  const profile = mkdtempSync(resolve(tmpdir(), 'exact-live-chrome-'));
  try {
    for (const exited of [Promise.resolve(), new Promise(() => {})]) {
      await expect((async () => {
        await quitBrowser({ pid: 42, exitCode: null, signalCode: null }, cdp, exited, { ms: 10, kill: () => {} });
        rmSync(profile, { recursive: true, force: true });
      })()).rejects.toThrow('Chrome 42 did not exit');
      expect(existsSync(profile)).toBe(true);
    }
  } finally { rmSync(profile, { recursive: true, force: true }); }
});

test('a shared CDP browser is replaced as soon as its exit rejects a pending request', () => {
  const run = spawnSync(process.execPath, ['-e', `
import { mock, expect } from 'bun:test';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
const processes = await import('node:child_process'), children = [], forced = [];
process.kill = (...args) => forced.push(args);
mock.module('node:child_process', () => ({ ...processes, spawn: () => {
  const child = new EventEmitter();
  child.pid = 1; child.exitCode = null; child.signalCode = null;
  child.stderr = new PassThrough();
  child.stdio = [null, null, child.stderr, new PassThrough(), new PassThrough()];
  child.finish = () => { child.exitCode = 0; child.emit('exit', 0); };
  child.stdio[3].on('data', data => {
    const msg = JSON.parse(String(data).replace(/\\0$/, ''));
    if (msg.method === 'Fixture.pending') return;
    if (msg.method === 'Browser.close') { child.finish(); return; }
    child.stdio[4].write(JSON.stringify({ id: msg.id, result: { browserContextId: String(msg.id) } }) + '\\0');
  });
  children.push(child); return child;
} }));
const { cdpBrowser } = await import(${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))});
const first = await cdpBrowser();
const pending = first.cdp.send('Fixture.pending');
children[0].finish();
await expect(pending).rejects.toThrow('Chrome exited');
const replacement = await cdpBrowser();
expect(replacement.cdp).not.toBe(first.cdp);
expect(children.length).toBe(2);
expect(forced).toEqual([]);
await replacement.close();
`], { env: { ...process.env, CHROME: process.execPath }, encoding: 'utf8', timeout: 5000 });
  expect(run.stderr).toBe('');
  expect(run.error).toBeUndefined();
  expect(run.status).toBe(0);
});

test('a shared CDP browser quits before its test process exits and removes its profile after termination', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-cdp-exit-'));
  const chrome = resolve(dir, 'chrome'), log = resolve(dir, 'browser.json');
  writeFileSync(chrome, `#!${process.execPath}
import { spawn } from 'node:child_process';
import { createReadStream, existsSync, writeSync, writeFileSync } from 'node:fs';
const log = process.env.BROWSER_LOG, mode = process.env.BROWSER_MODE;
const state = { pid: process.pid, profile: process.argv.find(a => a.startsWith('--user-data-dir=')).split('=')[1], methods: [] };
const save = () => writeFileSync(log, JSON.stringify(state));
save(); writeFileSync(log + '.clone', 'bundle');
let buffer = '';
createReadStream('', { fd: 3 }).on('data', chunk => {
  buffer += chunk;
  let end;
  while ((end = buffer.indexOf('\\0')) >= 0) {
    const msg = JSON.parse(buffer.slice(0, end)); buffer = buffer.slice(end + 1);
    state.methods.push(msg.method); save();
    if (msg.method === 'Browser.close') {
      if (mode !== 'hung') {
        const helper = spawn(process.execPath, ['-e', \`
import { rmSync } from 'node:fs';
const timer = setInterval(() => {
  if (process.ppid !== 1) return;
  rmSync(process.env.BROWSER_LOG + '.clone');
  clearInterval(timer);
}, 50);
\`], { stdio: 'ignore' });
        state.helper = helper.pid; state.clean = true; save(); helper.unref();
        setTimeout(() => {
          state.profileAtExit = existsSync(state.profile); save(); process.exit(0);
        }, mode === 'normal' ? 3500 : 0);
      }
    } else writeSync(4, JSON.stringify({ id: msg.id, ...(mode === 'startup'
      ? { error: { message: 'fixture refused context' } } : { result: { browserContextId: String(msg.id) } }) }) + '\\0');
  }
});
`);
  chmodSync(chrome, 0o755);
  const suite = resolve(dir, 'shutdown.test.mjs');
  writeFileSync(suite, `
import { test, expect } from 'bun:test';
import { EventEmitter } from 'node:events';
const once = EventEmitter.prototype.once;
let browser;
EventEmitter.prototype.once = function(event, ...args) { if (this.spawnfile && event === 'exit') browser = this; return once.call(this, event, ...args); };
import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
test('suite owns contexts on one browser', async () => {
  const [a, b] = await Promise.all([cdpBrowser(), cdpBrowser()]);
  expect(a.cdp).toBe(b.cdp);
  expect(a.browserContextId).not.toBe(b.browserContextId);
  if (process.env.BROWSER_MODE === 'error') browser.emit('error', new Error('a failed kill'));
  await a.close(); await b.close();
  expect(process.env.BROWSER_MODE).not.toBe('failure');
});
`);
  try {
    for (const mode of ['error', 'normal', 'failure', 'hung', 'startup', 'script', 'script-failure']) {
      const script = mode.startsWith('script');
      const args = script ? ['-e', `import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
        const suite = await cdpBrowser(); await suite.close(); process.exitCode = ${mode === 'script-failure' ? 1 : 0};`] : ['test', suite];
      const run = spawnSync(process.execPath, args, {
        env: { ...process.env, CHROME: chrome, BROWSER_LOG: log, BROWSER_MODE: mode }, encoding: 'utf8', timeout: 15000,
      });
      const state = JSON.parse(readFileSync(log, 'utf8'));
      try {
        expect(run.error).toBeUndefined();
        expect(run.status).toBe(['failure', 'startup', 'script-failure'].includes(mode) ? 1 : 0);
        const creates = script ? 1 : 2, disposes = mode === 'startup' ? 0 : creates;
        // After a failed kill the browser is closed first, and the contexts' disposals race its exit: either may reach it.
        const methods = mode === 'error' ? state.methods.filter(m => m !== 'Target.disposeBrowserContext') : state.methods;
        expect(methods).toEqual([...Array(creates).fill('Target.createBrowserContext'),
          ...(mode === 'error' ? [] : Array(disposes).fill('Target.disposeBrowserContext')), 'Browser.close']);
        expect(Boolean(state.clean)).toBe(mode !== 'hung');
        if (mode !== 'hung') {
          expect(state.profileAtExit).toBe(true);
          expect(await pollUntil(() => existsSync(log + '.clone'), value => !value, { bound: 3000 })).toBe(false);
        }
        expect(() => process.kill(state.pid, 0)).toThrow();
        expect(existsSync(state.profile)).toBe(false);
      } finally {
        try { process.kill(state.pid, 'SIGKILL'); } catch {}
        if (state.helper) { try { process.kill(state.helper, 'SIGKILL'); } catch {} }
        rmSync(state.profile, { recursive: true, force: true });
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 45000);

test('the benchmark handles a missing initial page, retries navigation and closes Chrome on failures', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-bench-close-'));
  const chrome = resolve(dir, 'chrome'), log = resolve(dir, 'browser.json');
  writeFileSync(chrome, `#!${process.execPath}
import { createReadStream, existsSync, writeSync, writeFileSync } from 'node:fs';
const state = { pid: process.pid, profile: process.argv.find(a => a.startsWith('--user-data-dir=')).slice(16), methods: [] };
const save = () => writeFileSync(process.env.BROWSER_LOG + '.' + process.pid, JSON.stringify(state));
let buffer = '', evaluations = 0;
save();
createReadStream('', { fd: 3 }).on('data', chunk => {
  buffer += chunk;
  let end;
  while ((end = buffer.indexOf('\\0')) >= 0) {
    const msg = JSON.parse(buffer.slice(0, end)); buffer = buffer.slice(end + 1);
    state.methods.push(msg.method); save();
    if (msg.method === 'Browser.close') { state.clean = true; state.profileAtExit = existsSync(state.profile); save(); process.exit(0); }
    let result = {};
    if (msg.method === 'Target.getTargets') result = { targetInfos: process.env.BROWSER_MODE === 'no-page' ? [] : [{ type: 'page', targetId: 'page' }] };
    if (msg.method === 'Target.createTarget') result = { targetId: 'created-page' };
    if (msg.method === 'Target.attachToTarget') result = { sessionId: 'session' };
    if (msg.method === 'Runtime.evaluate') result = { result: { value: JSON.stringify({ origin: Date.now(), done: true, runtime: true, content: 1, fcp: 1 }) } };
    const error = process.env.BROWSER_MODE === 'startup' ? 'fixture refused startup' :
      process.env.BROWSER_MODE === 'measurement' && msg.method === 'Runtime.evaluate' ? 'fixture evaluation failed' :
      msg.method === 'Runtime.evaluate' && evaluations++ === 0 ? 'Execution context was destroyed.' : null;
    writeSync(4, JSON.stringify({ id: msg.id, ...(error ? { error: { message: error } } : { result }) }) + '\\0');
  }
});
`);
  chmodSync(chrome, 0o755);
  try {
    for (const mode of ['no-page', 'navigation', 'startup', 'measurement']) {
      const succeeds = mode === 'no-page' || mode === 'navigation';
      const run = spawnSync(process.execPath, [resolve(import.meta.dir, '../host/web-js/bench.mjs'), 'fixture=http://127.0.0.1:9', '--profile', 'none', '--runs', '1'], {
        env: { ...process.env, CHROME: chrome, BROWSER_LOG: log + mode, BROWSER_MODE: mode }, encoding: 'utf8', timeout: 15000,
      });
      const states = readdirSync(dir).filter(name => name.startsWith('browser.json' + mode + '.'))
        .map(name => JSON.parse(readFileSync(resolve(dir, name), 'utf8')));
      try {
        expect(run.error).toBeUndefined();
        expect(run.status).toBe(succeeds ? 0 : 1);
        expect(states.length).toBe(succeeds ? 3 : 1);
        for (const state of states) {
          expect(state.methods.includes('Target.createTarget')).toBe(mode === 'no-page');
          expect(state.clean).toBe(true);
          expect(state.profileAtExit).toBe(true);
          expect(() => process.kill(state.pid, 0)).toThrow();
          expect(existsSync(state.profile)).toBe(false);
        }
      } finally {
        for (const state of states) { try { process.kill(state.pid, 'SIGKILL'); } catch {} }
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30000);

const branded = notApplicable(process.platform !== 'darwin' ? 'macOS only' :
  !/Google Chrome( Canary)?\.app\//.test(process.env.CHROME ?? '') ? 'CHROME must name branded Chrome' : null);
branded(
  'branded Chrome removes its bundle copies after script and test shutdown', async () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-branded-close-'));
    const temporary = spawnSync('getconf', ['DARWIN_USER_TEMP_DIR'], { encoding: 'utf8' }).stdout.trim();
    const clones = resolve(temporary, '../X', process.env.CHROME.includes('Canary') ? 'com.google.Chrome.canary.code_sign_clone' : 'com.google.Chrome.code_sign_clone');
    const names = () => existsSync(clones) ? readdirSync(clones) : [];
    const before = new Set(names()), added = () => names().filter(name => !before.has(name));
    const source = `import { test } from 'bun:test';
import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
test('browser', async () => { const b = await cdpBrowser(); await b.page(); await b.close(); if (process.env.BROWSER_FAIL) throw new Error('fixture failure'); });`;
    const suite = resolve(dir, 'branded.test.mjs');
    writeFileSync(suite, source);
    try {
      for (const mode of ['script', 'test', 'failure']) {
        const args = mode === 'script' ? ['-e', `import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))}; const b = await cdpBrowser(); await b.page(); await b.close();`] : ['test', suite];
        const run = spawnSync(process.execPath, args, { env: { ...process.env, BROWSER_FAIL: mode === 'failure' ? '1' : '' }, encoding: 'utf8', timeout: 20000 });
        expect(run.error).toBeUndefined();
        expect(run.status).toBe(mode === 'failure' ? 1 : 0);
        // Chrome's orphaned cleanup helper polls its parent once a second.
        expect(await pollUntil(added, value => value.length === 0, { bound: 5000, interval: 100 })).toEqual([]);
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 60000,
);
