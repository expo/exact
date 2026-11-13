// A page is served from the build it booted (generations.mjs; theme2's ruling on the build-swap pair): a page's
// files all come from one build while another replaces it, old builds are kept while their pages may ask and reaped
// as the ruling keeps them, and a generation path is checked strictly.
import { expect, spyOn, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { takeLock } from './build-lock.mjs';
import { buildTreeFile, replaceBuild, serveBuildTree } from './serve.mjs';
import { GENERATION, foreignBeside, generationFile, generationOf, generationPath, leaseGeneration, leasedGenerations, reapGenerations, servedFile } from './generations.mjs';
import { sitePages } from '../web-js/render.mjs';
import { BESIDE, STAGE, beside } from './beside.mjs';
import { fileURLToPath } from 'node:url';

/** A JS-target build naming `k` in every file: its document (base `/`) and the shell beside it, its module, a lazy
 * chunk named by `k` as a hashed chunk is, its plan; and its receipt. */
function build(dir, k) {
  mkdirSync(dir, { recursive: true });
  for (const page of ['index.html', 'shell.html']) writeFileSync(join(dir, page), `<!doctype html><html><head><base href="/"><script type="module" src="./app.js"></script></head><body>build ${k}</body></html>`);
  writeFileSync(join(dir, 'app.js'), `build ${k}`);
  writeFileSync(join(dir, `chunk-${k}.js`), `build ${k}`);
  writeFileSync(join(dir, 'app.plan'), `build ${k}`);
  writeFileSync(join(dir, '.exact-build.json'), JSON.stringify({ exactBuild: 1, target: 'js', files: [{ name: 'app.js', sha256: String(k) }] }));
  return dir;
}
const workspace = () => { const root = mkdtempSync(join(tmpdir(), 'exact-generations-')); return { root, dist: join(root, 'dist'), stage: (k) => build(join(root, `stage-${k}`), k) }; };
const text = (r) => (r.body == null ? null : Buffer.from(r.body).toString());
const baseOf = (html) => /<base href="([^"]+)">/.exec(html)?.[1];

// `sitePages`'s renderer, made with its generation's reader: a page of the shell and the module, each read as it renders.
const reading = (made = () => {}) => ({ pageServer: (read) => {
  made(read);
  return async () => ({ status: 200, headers: { 'cache-control': 'no-store' }, body: `${read('shell.html')}${read('app.js')}` });
} });
const serverBundle = (tree) => { mkdirSync(join(tree, '.gen')); writeFileSync(join(tree, '.gen/server.js'), ''); return tree; };
/** A swap's second half: the new build in place, the old kept as its generation, the lock released. */
const finishSwap = (dist, id, next, release) => { renameSync(next, dist); mkdirSync(dist + '.gen', { recursive: true }); renameSync(dist + '.previous', join(dist + '.gen', id)); release(); };

test('rendering waits across a rename gap for the selected generation', async () => {
  const { root, dist, stage } = workspace();
  let release = () => {}, wait;
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist), next = serverBundle(stage(2)), pages = sitePages(dist, reading());
    release = takeLock(dist + '.lock');
    renameSync(dist, dist + '.previous');
    // The swap is another process's: it finishes while this one waits.
    const atomicWait = Atomics.wait;
    wait = spyOn(Atomics, 'wait').mockImplementation((...args) => { if (existsSync(next)) finishSwap(dist, id, next, release); return atomicWait(...args); });
    const page = await pages(id, '/', () => undefined);
    expect(wait).toHaveBeenCalled();
    expect(page.html).toContain('build 1');
    expect(page.html).not.toContain('build 2');
  } finally { wait?.mockRestore(); release(); rmSync(root, { recursive: true, force: true }); }
});

test('rendering retries when retention lands after the missing-tree check and releases the lock', async () => {
  const { root, dist, stage } = workspace();
  let release = () => {}, check;
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist), next = serverBundle(stage(2)), retained = join(dist + '.gen', id);
    const pages = sitePages(dist, reading()), exists = fs.existsSync;
    release = takeLock(dist + '.lock');
    renameSync(dist, dist + '.previous');
    let landed = false;
    check = spyOn(fs, 'existsSync').mockImplementation(path => {
      const present = exists(path);
      if (path === join(retained, '.gen/server.js') && !landed) {
        expect(present).toBe(false);
        landed = true;
        finishSwap(dist, id, next, release);
      }
      return present;
    });
    const page = await pages(id, '/', () => undefined);
    expect(landed).toBe(true);
    expect(page.html).toContain('build 1');
    expect(page.html).not.toContain('build 2');
  } finally { check?.mockRestore(); release(); rmSync(root, { recursive: true, force: true }); }
});

// A swap lands during a read, and crosses the wait's second as it does: the read is made again where the generation
// went, never answered from the build that replaced it.
test.each([['construction', 'reads'], ['construction', 'fails'], ['render', 'reads'], ['render', 'fails']])('a %s read a swap interrupts, which then %s, is read again from its generation', async (where, outcome) => {
  const { root, dist, stage } = workspace();
  let read, clock;
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist), next = serverBundle(stage(2)), original = fs.readFileSync;
    let now = Date.now(), armed = where === 'construction', swapped = false;
    clock = spyOn(Date, 'now').mockImplementation(() => now);
    read = spyOn(fs, 'readFileSync').mockImplementation((path, ...rest) => {
      if (armed && !swapped && path === join(dist, 'shell.html')) {
        swapped = true;
        replaceBuild(next, dist);
        now += 1200;
        if (outcome === 'fails') throw Object.assign(new Error('ENOENT: the build moved during a read'), { code: 'ENOENT' });
      }
      return original(path, ...rest);
    });
    const pages = sitePages(dist, reading(read => { if (where === 'construction') read('shell.html'); else armed = true; }));
    const page = await pages(id, '/', () => undefined);
    expect(swapped).toBe(true);
    expect(page.html).toContain('build 1');
    expect(page.html).not.toContain('build 2');
  } finally { read?.mockRestore(); clock?.mockRestore(); rmSync(root, { recursive: true, force: true }); }
});

test('a read that fails between the renames of a byte-identical rebuild is read again', async () => {
  const { root, dist, stage } = workspace();
  let read;
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist), original = fs.readFileSync;
    let swapped = false;
    read = spyOn(fs, 'readFileSync').mockImplementation((path, ...rest) => {
      if (!swapped && path === join(dist, 'app.js')) {
        // The whole swap, lock released, before the failed read is looked at: the same generation, another tree.
        swapped = true;
        const release = takeLock(dist + '.lock');
        renameSync(dist, dist + '.previous');
        renameSync(serverBundle(build(join(root, 'again'), 1)), dist);
        rmSync(dist + '.previous', { recursive: true });
        release();
        throw Object.assign(new Error('ENOENT: the read fell between the renames'), { code: 'ENOENT' });
      }
      return original(path, ...rest);
    });
    const page = await sitePages(dist, reading())(id, '/', () => undefined);
    expect([swapped, generationOf(dist)]).toEqual([true, id]);
    expect(page.html).toContain('build 1');
  } finally { read?.mockRestore(); rmSync(root, { recursive: true, force: true }); }
});

test('a file the generation does not have, or a bundle that fails, fails the render at once', async () => {
  const { root, dist, stage } = workspace();
  let wait;
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist);
    wait = spyOn(Atomics, 'wait');
    const missing = sitePages(dist, { pageServer: (read) => async () => ({ status: 200, headers: {}, body: read('missing.js') }) });
    await expect(missing(id, '/', () => undefined)).rejects.toThrow('ENOENT');
    const broken = sitePages(dist, { pageServer: () => { throw new Error('invalid bundle'); } });
    await expect(broken(id, '/', () => undefined)).rejects.toThrow('invalid bundle');
    expect(wait).not.toHaveBeenCalled();
  } finally { wait?.mockRestore(); rmSync(root, { recursive: true, force: true }); }
});

test('a selected build without a server bundle gets its shell while cleanup holds the lock', async () => {
  const { root, dist, stage } = workspace();
  let release = () => {}, wait;
  try {
    replaceBuild(stage(1), dist);
    release = takeLock(dist + '.lock');
    wait = spyOn(Atomics, 'wait');
    expect(await sitePages(dist)(generationOf(dist), '/', () => undefined)).toBe(null);
    expect(wait).not.toHaveBeenCalled();
  } finally { wait?.mockRestore(); release(); rmSync(root, { recursive: true, force: true }); }
});

test('an unfinished generation swap returns a bounded retry response instead of the shell', async () => {
  const { root, dist, stage } = workspace();
  let release = () => {}, clock;
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist), pages = sitePages(dist, reading());
    release = takeLock(dist + '.lock');
    renameSync(dist, dist + '.previous');
    // The second passes at the first look: no wait on the wall clock.
    const now = Date.now();
    clock = spyOn(Date, 'now').mockReturnValueOnce(now).mockReturnValue(now + 1200);
    expect(await pages(id, '/', () => undefined)).toMatchObject({ status: 503, retryAfter: '1', cacheControl: 'no-store' });
  } finally { clock?.mockRestore(); release(); rmSync(root, { recursive: true, force: true }); }
});

test('a generation no longer kept, with no swap under way, gets its shell', async () => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(serverBundle(stage(1)), dist);
    const id = generationOf(dist);
    replaceBuild(serverBundle(stage(2)), dist);
    rmSync(join(dist + '.gen', id), { recursive: true });
    expect(await sitePages(dist, reading())(id, '/', () => undefined)).toBe(null);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a lease released while a replacement checks what is beside the build does not stop it', () => {
  const { root, dist, stage } = workspace();
  let list;
  try {
    replaceBuild(stage(1), dist);
    const release = leaseGeneration(dist, generationOf(dist)), names = fs.readdirSync;
    list = spyOn(fs, 'readdirSync').mockImplementation((dir, ...rest) => {
      const found = names(dir, ...rest);
      if (String(dir).startsWith(dist + '.leases/')) release();
      return found;
    });
    replaceBuild(stage(2), dist);
    expect(readFileSync(join(dist, 'app.js'), 'utf8')).toBe('build 2');
    // A foreign entry that stays is still refused.
    list.mockRestore();
    mkdirSync(join(dist + '.leases', 'photos'), { recursive: true });
    expect(() => replaceBuild(stage(3), dist)).toThrow('is there and is not');
  } finally { list?.mockRestore(); rmSync(root, { recursive: true, force: true }); }
});

test.each(['previous', 'gen'])('a foreign .%s created after the output check survives replacement', (sibling) => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist);
    expect(foreignBeside(dist)).toBe(null);
    const next = stage(2), foreign = sibling === 'previous' ? dist + '.previous' : join(dist + '.gen', 'photos');
    mkdirSync(foreign, { recursive: true });
    writeFileSync(join(foreign, 'keep.txt'), 'mine');
    expect(() => replaceBuild(next, dist)).toThrow('is there and is not');
    expect(readFileSync(join(foreign, 'keep.txt'), 'utf8')).toBe('mine');
    expect(readFileSync(join(dist, 'app.js'), 'utf8')).toBe('build 1');
    expect(existsSync(next)).toBe(true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test.each(['previous', 'gen'])('a foreign .%s created after the first build lands survives cleanup', (sibling) => {
  const { root, dist, stage } = workspace();
  let swap;
  try {
    const next = stage(1), foreign = sibling === 'previous' ? dist + '.previous' : join(dist + '.gen', 'photos');
    const rename = fs.renameSync;
    let landed = false;
    swap = spyOn(fs, 'renameSync').mockImplementation((from, to) => {
      rename(from, to);
      if (from === next && to === dist) {
        landed = true;
        mkdirSync(foreign, { recursive: true });
        writeFileSync(join(foreign, 'keep.txt'), 'mine');
      }
    });
    expect(() => replaceBuild(next, dist)).toThrow('is there and is not');
    expect(landed).toBe(true);
    expect(readFileSync(join(foreign, 'keep.txt'), 'utf8')).toBe('mine');
    expect(readFileSync(join(dist, 'app.js'), 'utf8')).toBe('build 1');
    expect(existsSync(next)).toBe(false);
  } finally { swap?.mockRestore(); rmSync(root, { recursive: true, force: true }); }
});

test.each([['photos', false], ['f'.repeat(32), false], ['photos', true]])('reaping preserves foreign %s, created during reaping: %s', (name, during) => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist);
    replaceBuild(stage(2), dist);
    expect(foreignBeside(dist)).toBe(null);
    const foreign = join(dist + '.gen', name);
    const makeForeign = () => { mkdirSync(foreign); writeFileSync(join(foreign, 'keep.txt'), 'mine'); };
    const before = readdirSync(dist + '.gen').concat(name).sort();
    if (!during) makeForeign();
    const keep = { get newest() { if (during) makeForeign(); return 0; }, ageMs: 0, bytes: 0 };
    expect(() => reapGenerations(dist, keep)).toThrow('is there and is not');
    expect(readFileSync(join(foreign, 'keep.txt'), 'utf8')).toBe('mine');
    expect(readdirSync(dist + '.gen').sort()).toEqual(before);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test.each(['age', 'bytes'])('reaping records completed deletions when foreign entries interrupt the %s pass', async (pass) => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist);
    const first = generationOf(dist);
    replaceBuild(stage(2), dist);
    const second = generationOf(dist);
    replaceBuild(stage(3), dist);
    for (const [i, id] of [first, second].entries()) {
      const time = new Date(1000 + i);
      utimesSync(join(dist + '.gen', id), time, time);
    }
    const foreign = join(dist + '.gen', 'photos');
    let decisions = 0;
    const interrupt = () => {
      if (++decisions !== 2) return;
      mkdirSync(foreign); writeFileSync(join(foreign, 'keep.txt'), 'mine');
    };
    const keep = {
      get newest() { if (pass === 'age') interrupt(); return pass === 'age' ? 0 : 2; },
      ageMs: 0,
      get bytes() { if (pass === 'bytes') interrupt(); return 0; },
    };
    expect(() => reapGenerations(dist, keep)).toThrow('is there and is not');
    const removed = pass === 'age' ? second : first, retained = pass === 'age' ? first : second;
    expect(existsSync(join(dist + '.gen', removed))).toBe(false);
    expect((await servedFile(dist, `/.exact/gen/${removed}/app.js`)).status).toBe(410);
    expect(text(await servedFile(dist, `/.exact/gen/${retained}/app.js`))).toBe(`build ${retained === first ? 1 : 2}`);
    expect(readFileSync(join(foreign, 'keep.txt'), 'utf8')).toBe('mine');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test.each([[false, false], [false, true], [true, false], [true, true]])('render --serve shares file resolution, with server bundle %s and receipt %s', async (bundle, receipt) => {
  const { root, dist } = workspace();
  let child;
  try {
    build(dist, 1);
    if (!receipt) rmSync(join(dist, '.exact-build.json'));
    mkdirSync(join(dist, 'about')); writeFileSync(join(dist, 'about/index.html'), '<base href="/"><p>About</p>');
    writeFileSync(join(dist, 'routes.json'), JSON.stringify({ routes: [{ pattern: '/about', render: 'build' }, { pattern: '/client', render: 'client' }] }));
    if (bundle) serverBundle(dist);
    const probe = createServer();
    await new Promise(ok => probe.listen(0, '127.0.0.1', ok));
    const port = probe.address().port;
    await new Promise(ok => probe.close(ok));
    child = Bun.spawn([process.execPath, join(import.meta.dir, '../web-js/render.mjs'), dist, '--serve', '--port', String(port)], { stdout: 'pipe', stderr: 'pipe' });
    const errors = new Response(child.stderr).text();
    const reader = child.stdout.getReader();
    const ready = (async () => {
      let output = '';
      for (;;) {
        const { value, done } = await reader.read();
        if (done) throw new Error('render server exited: ' + await errors);
        output += new TextDecoder().decode(value);
        if (output.includes('serving')) return 'http://127.0.0.1:' + port + '/';
      }
    })();
    const url = await ready;
    for (const [path, status, body] of [['about/', 200, '<p>About</p>'], ['client', 200, 'build 1'], ['.gen/server.js', 404, '']]) {
      const r = await fetch(new URL(path, url), { headers: { accept: 'text/html' } });
      expect([path, r.status, (await r.text()).includes(body)]).toEqual([path, status, true]);
    }
  } finally {
    if (child) { child.kill(); await child.exited; }
    rmSync(root, { recursive: true, force: true });
  }
});

test('a page booted on one build gets that build\'s files after another lands; the path by name mixes them (the negative control)', async () => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist);
    // The page: its document names its generation in its base.
    const page = await servedFile(dist, '/', 'text/html');
    expect([page.status, text(page)]).toEqual([200, expect.stringContaining('build 1')]);
    const base = baseOf(text(page));
    expect(base).toBe(`/.exact/gen/${generationOf(dist)}/`);
    replaceBuild(stage(2), dist);
    // Build 2 in place: the page's module, chunk and plan are still build 1's, from the kept tree.
    for (const file of ['app.js', 'chunk-1.js', 'app.plan']) expect(text(await servedFile(dist, base + file))).toBe('build 1');
    // The path by name, as servers read it before: the module is build 2's now, and the chunk gone (the mix).
    expect(readFileSync(buildTreeFile(dist, '/app.js').path, 'utf8')).toBe('build 2');
    expect(buildTreeFile(dist, '/chunk-1.js')).toBe(null);
    // A new page boots build 2.
    expect(baseOf(text(await servedFile(dist, '/', 'text/html')))).toBe(`/.exact/gen/${generationOf(dist)}/`);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a page\'s files all come from one build while 100 builds replace it', async () => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(0), dist);
    let k = 0, done = false, mixed = 0, pages = 0;
    // Each build waits its turn in a stage of its own, made before the swap (as the builds' are).
    const writer = (async () => { while (k < 100) { const s = stage(++k); replaceBuild(s, dist); await new Promise((ok) => setTimeout(ok, 1)); } done = true; })();
    while (!done) {
      const page = await servedFile(dist, '/', 'text/html');
      const booted = /build (\d+)/.exec(text(page))[1], base = baseOf(text(page));
      const files = await Promise.all(['app.js', `chunk-${booted}.js`, 'app.plan'].map((f) => servedFile(dist, base + f)));
      // A page whose build was reaped meanwhile is told so (410), never served another build's file.
      for (const f of files) if (f.status !== 410 && text(f) !== `build ${booted}`) mixed++;
      pages++;
      await new Promise((ok) => setTimeout(ok, 0)); // (the writer's turn: these reads resolve without one)
    }
    await writer;
    expect(pages).toBeGreaterThan(10);
    expect(mixed).toBe(0);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('builds are kept by generation and reaped as the ruling keeps them: the newest 3, the leased, the young, within the cap', async () => {
  const { root, dist, stage } = workspace();
  try {
    const ids = [];
    for (let k = 1; k <= 6; k++) { replaceBuild(stage(k), dist); ids.push(generationOf(dist)); }
    // Builds 1 to 5 were replaced; all are under 15 minutes old, so all are kept.
    const kept = () => readdirSync(`${dist}.gen`).filter((n) => GENERATION.test(n)).sort();
    expect(kept()).toEqual(ids.slice(0, 5).sort());
    // Aged past 15 minutes: only the newest 3 stay, and a leased one, and nothing a dead process leased.
    const old = new Date(Date.now() - 20 * 60_000);
    for (const id of ids.slice(0, 5)) utimesSync(join(`${dist}.gen`, id), old, old);
    const release = leaseGeneration(dist, ids[0]);
    const dead = spawnSync(process.execPath, ['-e', '0']).pid ?? 999999;
    mkdirSync(join(`${dist}.leases`, ids[1]), { recursive: true });
    writeFileSync(join(`${dist}.leases`, ids[1], String(dead)), 'a start long gone');
    expect([...leasedGenerations(dist)]).toEqual([ids[0]]);
    // (The newest by when each was put aside: all aged alike, so by name order the three kept are any three; give
    // them distinct ages.)
    for (const [i, id] of ids.slice(0, 5).entries()) { const at = new Date(Date.now() - (30 - i) * 60_000); utimesSync(join(`${dist}.gen`, id), at, at); }
    const reaped = reapGenerations(dist);
    expect(kept()).toEqual([ids[0], ids[2], ids[3], ids[4]].sort());
    expect(reaped).toEqual([ids[1]]);
    // A reaped page is told its build is gone (410); an id that names nothing here is not found (404).
    expect((await servedFile(dist, `/.exact/gen/${ids[1]}/app.js`)).status).toBe(410);
    expect((await servedFile(dist, `/.exact/gen/${'0'.repeat(32)}/app.js`)).status).toBe(404);
    // The lease ended: reaped at the next pass. The cap: the oldest unleased first.
    release();
    expect(reapGenerations(dist)).toEqual([ids[0]]);
    const one = readdirSync(join(`${dist}.gen`, ids[4])).reduce((n, f) => n + statSync(join(`${dist}.gen`, ids[4], f)).size, 0);
    expect(reapGenerations(dist, { newest: 3, ageMs: 15 * 60_000, bytes: one })).toEqual([ids[2], ids[3]]);
    const again = leaseGeneration(dist, ids[4]);
    expect(reapGenerations(dist, { newest: 0, ageMs: 0, bytes: 0 })).toEqual([]);
    again();
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a generation path is checked strictly: its id\'s form, no dot segment or encoded slash, nothing read outside', async () => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist);
    const id = generationOf(dist);
    writeFileSync(join(root, 'secret'), 'outside');
    for (const path of [`/.exact/gen/${id}/../../secret`, `/.exact/gen/${id}/%2e%2e/secret`, `/.exact/gen/${id}/a%2fb`, `/.exact/gen/${id.slice(1)}/app.js`,
      `/.exact/gen/${id.toUpperCase()}/app.js`, `/.exact/gen/${id}`, `/.exact/gen/../dist/app.js`]) {
      const r = await servedFile(dist, path);
      expect([path, r.status]).toEqual([path, 400]);
    }
    expect(generationPath(`/.exact/gen/${id}/app.js`)).toEqual({ id, path: '/app.js' });
    expect(generationPath('/app.js')).toBe(null);
    // A page the build has no file or route for is a miss: 404 with the shell (the client draws its notfound
    // route), never a file outside it.
    const outside = await servedFile(dist, `/.exact/gen/${id}/secret`, 'text/html');
    expect([outside.status, outside.route, text(outside).includes('outside')]).toEqual([404, '/shell.html', false]);
    expect(text(await servedFile(dist, `/.exact/gen/${id}/app.js`))).toBe('build 1');
    // Under another base (a server that serves the build under a path): the same, under that path.
    expect(generationPath(`/tool/.exact/gen/${id}/app.js`, '/tool/')).toEqual({ id, path: '/app.js' });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('leases: each drive its own (two in one process), a live process whose start `ps` cannot say or another user\'s kept', () => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist);
    const id = generationOf(dist), a = leaseGeneration(dist, id), b = leaseGeneration(dist, id);
    a();
    expect([...leasedGenerations(dist)]).toEqual([id]); // the other drive's still holds it
    b();
    expect([...leasedGenerations(dist)]).toEqual([]);
    // A start `ps` could not say (written empty) of a live process; another user's process (EPERM): kept.
    mkdirSync(join(`${dist}.leases`, id), { recursive: true });
    writeFileSync(join(`${dist}.leases`, id, `${process.pid}-x`), '');
    expect([...leasedGenerations(dist)]).toEqual([id]);
    rmSync(join(`${dist}.leases`, id, `${process.pid}-x`));
    writeFileSync(join(`${dist}.leases`, id, '1-x'), '');
    expect([...leasedGenerations(dist)]).toEqual([id]);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a build reaped while a page asks: told gone (410) once the swap is done, not a passing 404', async () => {
  const { root, dist, stage } = workspace();
  try {
    replaceBuild(stage(1), dist); const old = generationOf(dist);
    replaceBuild(stage(2), dist);
    // A swap under way (this process holds the lock), the kept tree's files going first.
    const release = takeLock(`${dist}.lock`);
    rmSync(join(`${dist}.gen`, old, 'app.js'));
    const asked = generationFile(dist, old, '/app.js');
    await new Promise((ok) => setTimeout(ok, 50));
    rmSync(join(`${dist}.gen`, old), { recursive: true, force: true });
    writeFileSync(`${dist}.gen.reaped`, old + '\n');
    release();
    expect(await asked).toBe('gone');
    // A lock file a killed build left (naming its pid, held by no one) holds nothing up.
    writeFileSync(`${dist}.lock`, String(spawnSync(process.execPath, ['-e', '0']).pid));
    const at = Date.now();
    expect(await generationFile(dist, '1'.repeat(32), '/app.js')).toBe(null);
    expect(Date.now() - at).toBeLessThan(500);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('with two renames for a swap (no one-step exchange), a page\'s files are still one build\'s and no page goes unpinned', () => {
  // A process of its own: the exchange is chosen once a process.
  const r = spawnSync(process.execPath, ['-e', `
    const { mkdtempSync, mkdirSync, writeFileSync } = await import('node:fs'); const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
    const { replaceBuild } = await import(${JSON.stringify(fileURLToPath(new URL('./serve.mjs', import.meta.url)))});
    const { servedFile } = await import(${JSON.stringify(fileURLToPath(new URL('./generations.mjs', import.meta.url)))});
    const root = mkdtempSync(join(tmpdir(), 'exact-generations-renames-')), dist = join(root, 'dist');
    const build = (k) => { const d = join(root, 's' + k); mkdirSync(d); writeFileSync(join(d, 'index.html'), '<base href="/"><p>build ' + k + '</p>'); writeFileSync(join(d, 'app.js'), 'build ' + k);
      writeFileSync(join(d, '.exact-build.json'), JSON.stringify({ exactBuild: 1, target: 'js', files: [{ name: 'app.js', sha256: String(k) }] })); return d; };
    replaceBuild(build(0), dist);
    let k = 0, done = false, bad = 0, pages = 0;
    const writer = (async () => { while (k < 60) { replaceBuild(build(++k), dist); await new Promise((ok) => setTimeout(ok, 1)); } done = true; })();
    while (!done) {
      const page = await servedFile(dist, '/', 'text/html');
      if (page.status === 503) { await new Promise((ok) => setTimeout(ok, 0)); continue; }
      const text = Buffer.from(page.body).toString(), base = /<base href="([^"]+)">/.exec(text)[1], booted = /build (\\d+)/.exec(text)[1];
      if (base === '/') bad++;
      const js = await servedFile(dist, base + 'app.js');
      if (js.status !== 410 && Buffer.from(js.body).toString() !== 'build ' + booted) bad++;
      pages++; await new Promise((ok) => setTimeout(ok, 0));
    }
    await writer;
    console.log(JSON.stringify({ bad, pages }));
  `], { encoding: 'utf8', env: { ...process.env, EXACT_DIRECTORY_EXCHANGE: '0' } });
  expect(r.status, r.stderr).toBe(0);
  const { bad, pages } = JSON.parse(r.stdout.trim().split('\n').at(-1));
  expect(pages).toBeGreaterThan(5);
  expect(bad).toBe(0);
});

test('no build yet: not found at once, not held for a swap that is not under way', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-generations-none-'));
  try {
    const at = Date.now();
    expect((await servedFile(join(root, 'dist'), '/', 'text/html')).status).toBe(404);
    expect(Date.now() - at).toBeLessThan(500);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// LLP 1153 D3: what a build keeps beside its directory is the build's to replace and reap, so beside an author's
// `--out` directory each such name must be absent or a build's own (`foreignBeside`); and the native bake's capture
// skips the same names as the web's scans (js/bake/src/beside.rs, host/web/beside.mjs).
test('what a build keeps beside its directory is its own, or the directory is refused; the bake skips the same names', () => {
  const { root, dist, stage } = workspace();
  try {
    expect(foreignBeside(dist)).toBe(null);
    // A build's own, after it replaced and reaped others: a generation kept, the ones reaped, a lease, a lock.
    for (const k of [1, 2, 3, 4, 5]) replaceBuild(stage(k), dist);
    for (const id of readdirSync(`${dist}.gen`)) { const t = new Date(0); utimesSync(join(`${dist}.gen`, id), t, t); }
    reapGenerations(dist, { newest: 1, ageMs: 0, bytes: 2 ** 30 });
    const release = leaseGeneration(dist, readdirSync(`${dist}.gen`)[0]);
    writeFileSync(`${dist}.lock`, String(process.pid));
    mkdirSync(`${dist}.previous`); writeFileSync(`${dist}.previous/.exact-build.json`, '{}');
    expect(readFileSync(`${dist}.gen.reaped`, 'utf8').trim().split('\n').every((line) => GENERATION.test(line))).toBe(true);
    expect(foreignBeside(dist)).toBe(null);
    // Someone else's, under each name in turn: refused, naming it.
    for (const [name, make] of [['previous', (p) => { mkdirSync(p); writeFileSync(join(p, 'notes.txt'), 'x'); }],
      ['gen', (p) => mkdirSync(join(p, 'photos'), { recursive: true })], ['gen.reaped', (p) => writeFileSync(p, 'not an id\n')],
      ['leases', (p) => mkdirSync(join(p, 'photos'), { recursive: true })], ['lock', (p) => writeFileSync(p, 'mine')]]) {
      const path = `${dist}.${name}`, aside = `${path}.aside`;
      renameSync(path, aside);
      make(path);
      expect(foreignBeside(dist), name).toStartWith(`${path} is there and is not`);
      rmSync(path, { recursive: true, force: true });
      renameSync(aside, path);
    }
    expect(foreignBeside(dist)).toBe(null);
    release();
    // The two lists of names agree.
    const rust = readFileSync(new URL('../../js/bake/src/beside.rs', import.meta.url), 'utf8');
    expect(JSON.parse(/BESIDE: \[&str; \d+\] = (\[[^\]]*\]);/.exec(rust)[1])).toEqual(BESIDE);
    expect(/STAGE: &str = "([^"]*)";/.exec(rust)[1]).toBe(STAGE);
    for (const name of [...BESIDE.map((n) => `dist.${n}`), `dist.${STAGE}a1b2c3`]) expect(beside(name), name).toBe(true);
    for (const name of ['dist', `dist.${STAGE}`, 'dist.config.ts', 'distance.json']) expect(beside(name), name).toBe(false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test('Markdown home pages and the missing page negotiate within current and retained generations', async () => {
  const { root, dist, stage } = workspace();
  const server = createServer((req, res) => serveBuildTree(dist, req, res));
  try {
    const first = stage(1); writeFileSync(join(first, 'index.md'), '# First\n'); writeFileSync(join(first, '404.md'), '# First missing\n');
    replaceBuild(first, dist);
    const before = generationOf(dist);
    const second = stage(2); writeFileSync(join(second, 'index.md'), '# Second\n'); writeFileSync(join(second, '404.md'), '# Second missing\n');
    replaceBuild(second, dist);
    await new Promise(ok => server.listen(0, '127.0.0.1', ok));
    const seen = [];
    for (const id of [generationOf(dist), before]) for (const path of ['', 'no/such/route']) {
      const r = await fetch(`http://127.0.0.1:${server.address().port}/.exact/gen/${id}/${path}`, { headers: { accept: 'text/markdown' } });
      seen.push([r.status, r.headers.get('content-type'), r.headers.get('vary'), await r.text()]);
    }
    expect(seen).toEqual([
      [200, 'text/markdown; charset=utf-8', 'Accept', '# Second\n'],
      [404, 'text/markdown; charset=utf-8', 'Accept', '# Second missing\n'],
      [200, 'text/markdown; charset=utf-8', 'Accept', '# First\n'],
      [404, 'text/markdown; charset=utf-8', 'Accept', '# First missing\n'],
    ]);
  } finally {
    server.closeAllConnections(); await new Promise(ok => server.close(ok));
    rmSync(root, { recursive: true, force: true });
  }
});
