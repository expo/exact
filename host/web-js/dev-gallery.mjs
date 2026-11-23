// On-demand pages on the JS dev server. The gallery (LLP 1139 D5) builds
// generated library examples. Frames build authored entries with the app's
// data. Each preview owns its build, error stream and open pages, and stops
// after its last page disconnects.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { builtSettings, sourceInputChanges } from '../../js/bake/src/__exact_env.mjs';
import { changedContractReads, contractReadReceipt } from '../../scripts/web-build.mjs';
import { within } from '../../scripts/paths.mjs';
import { compilerFrom, freshCompiler, toolEnv } from '../../scripts/tools.mjs';
import { watchDirectoryInputs } from '../web/dev-watch.mjs';
import { buildFileCards, buildTreeFileAsync as buildTreeFile, copyStaticTreeIfPresent, replaceBuild, sendStaticBody, webContentType } from '../web/serve.mjs';
import { EXE, frameFixturePath, frameURL } from './module.mjs';

/** The gallery's build cache: under the app's nearest `node_modules` (which
 * package resolution reaches, and git and describe's files don't), else the
 * repo's `target`, keyed by the app's id (one `node_modules` may serve many
 * apps of a workspace). */
export function galleryCache(app, root) {
  for (let dir = app.dir; ; dir = dirname(dir)) {
    if (existsSync(resolve(dir, 'node_modules'))) return resolve(dir, 'node_modules/.cache/exact-gallery', app.id);
    if (dirname(dir) === dir) return resolve(root, 'target/dev-gallery', app.id);
  }
}

const json = (frame) => `data: ${JSON.stringify(frame)}\n\n`;
const html = (text) => text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
// Before its first build, a page that waits on the gallery's stream and loads again when one lands.
const waiting = (label, stream, error) => `<!doctype html><meta charset="utf-8"><title>${html(label)}</title><pre id="g" style="font:12px/1.4 ui-monospace,monospace;padding:12px">${html(error || 'Building ' + label + '…')}</pre><script>new EventSource('${stream}').onmessage=e=>{const m=JSON.parse(e.data);if(m.error)document.getElementById('g').textContent=m.error;else if(m.reload)location.reload()}</script>`;
const MAPS = 4;

/** The gallery for one loop. `client(n)` is the dev client script for a
 * page of build `n` (the loop's own, with the gallery's stream, design and
 * page); `channel()` the build channel; `appDist` the app's dist (the
 * gallery's assets are the app's, served from there); `tableTool()` the
 * command that makes a plan's inspect table, as the loop makes the app's
 * (`{cmd, pre}`); `compiler()` the loop's fresh JS-target compiler, if any;
 * `design()` the app's committed design digest. Answers its routes
 * (`handle`), hears that the app published (`appChanged`, `designKept`, each
 * called after the app's own publication), keeps its maps (`map`), and stops
 * (`stop`), every process it started with it. */
export function galleryLoop(options) {
  return previewLoop({ ...options, base: '/__dev/gallery', label: 'the gallery', cache: galleryCache(options.app, options.root) });
}

// Every running preview shares one process-exit listener, including when many frames are open.
const children = new Set();
function killChildren() {
  for (const child of children) { try { process.kill(-child.pid, 'SIGKILL'); } catch { /* gone */ } }
}
function previewLoop({ app, root, appDist, base, label, cache, frame, inputs, onIdle, channel = () => null, client, tableTool = () => null, compiler = () => null, design: designNow = () => null, env = process.env, log = console.log, idleMs = 60_000 }) {
  const controls = frame ? base + '/__dev' : base;
  const source = resolve(cache, 'source'), live = resolve(cache, 'live'), stage = resolve(cache, 'stage'), dist = resolve(cache, 'dist');
  const clients = new Set(), maps = new Map(), tables = new Map(), procs = new Set();
  // `seq` only grows (a page that reconnects compares against it); `active` while a page has asked since the last
  // idle stop; `run` counts stops, so what a stopped run finishes says nothing.
  let seq = 0, built = false, active = false, failed = null, busy = false, again = false, idle = null, run = 0, design = null;

  let fingerprint = null, timer = null;
  const schedule = () => {
    if (!active) return;
    clearTimeout(timer);
    timer = setTimeout(() => { timer = null; if (active) refresh(); }, 5);
  };

  const push = (frame) => { for (const res of clients) res.write(json(frame)); };
  const shown = frame ? label : 'gallery';
  const fail = (error) => { failed = error; log(`${shown}: ${String(error).split('\n')[0]}`); push({ error: String(error) }); };

  // If LLP 1174 D5's compiler search reaches Cargo, build in the gallery's own target to avoid taking the app's lock.
  const target = env.CARGO_TARGET_DIR ?? resolve(root, 'target'), contractBin = resolve(target, `debug/contract${EXE}`);

  /** `cmd args`, its output, in a process group of its own, which the gallery owns (`procs`) until the whole
   * group is gone (the builder's own children, its `git`, outlive it a moment), killed after 5 s more. */
  function step(cmd, args, options) {
    return new Promise((done) => {
      const c = spawn(cmd, args, { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], detached: true, ...options });
      procs.add(c);
      if (!children.size) process.once('exit', killChildren);
      children.add(c);
      const forget = () => {
        procs.delete(c); children.delete(c);
        if (!children.size) process.removeListener('exit', killChildren);
      };
      let output = '';
      c.stdout?.on('data', (d) => { output += d; });
      c.stderr?.on('data', (d) => { output += d; });
      const gone = (code, until = Date.now() + 5000) => {
        try { process.kill(-c.pid, 0); } catch (e) { if (e.code === 'ESRCH') { forget(); return done({ code, output }); } }
        if (Date.now() > until) { try { process.kill(-c.pid, 'SIGKILL'); } catch { /* gone */ } }
        setTimeout(() => gone(code, until), 20);
      };
      c.on('error', (e) => { forget(); done({ code: -1, output: String(e) }); });
      c.on('exit', (code) => gone(code));
    });
  }

  async function generateGallery(mine) {
    const selected = compilerFrom('contract', contractBin, env);
    let made = selected ? await step(selected.path, ['gallery', app.dir, '-o', source], { env: toolEnv(env) }) : { code: 2 };
    if (mine !== run) return;
    if (made.code === 2) made = await step('cargo', ['run', '-q', '-p', 'contract', '--', 'gallery', app.dir, '-o', source], { env: { ...env, CARGO_TARGET_DIR: resolve(cache, 'cargo') } });
    if (mine !== run) return;
    if (made.code !== 0) throw new Error(made.output.trim() || `contract gallery exited ${made.code}`);
    mkdirSync(live, { recursive: true });
    for (const name of ['app.contract', 'app.json']) {
      const next = readFileSync(resolve(source, name));
      if (!existsSync(resolve(live, name)) || !next.equals(readFileSync(resolve(live, name)))) writeFileSync(resolve(live, name), next);
    }
    // The generated source links the app's assets for compilation; the JS builder requires real static trees.
    const assets = resolve(live, 'assets');
    rmSync(assets, { recursive: true, force: true });
    copyStaticTreeIfPresent(resolve(app.dir, 'assets'), assets);
    log(made.output.trim().split('\n').pop());
  }

  /** Publish a complete preview and notify the pages waiting on its stream. */
  async function refresh() {
    if (busy) { again = true; return; }
    busy = true;
    const mine = run;
    // The design it builds on: one kept since is news, this one isn't. A failure before is taken up by this run.
    design = designNow();
    const previousError = failed;
    try {
      if (!frame) await generateGallery(mine);
      if (mine !== run) return;
      // The JS target's build, rendering no pages, into its own stage: nothing of the app's build is shared.
      rmSync(stage, { recursive: true, force: true });
      const started = Date.now(), webCompiler = env.EXACT_WEB_COMPILER || compiler();
      const entry = frame ? [app.dir, '--frame', frame, '--serve-frame'] : [live];
      const buildEnv = { ...env };
      if (webCompiler) buildEnv.EXACT_WEB_COMPILER = webCompiler;
      if (!frame && !(webCompiler && freshCompiler(webCompiler)) &&
        !compilerFrom('exact-web-js', resolve(target, `debug/exact-web-js${EXE}`), env)) {
        buildEnv.CARGO_TARGET_DIR = resolve(cache, 'cargo');
      }
      const out = await step(process.execPath, [resolve(root, 'host/web-js/build.mjs'), ...entry, '--out', stage, '--render', 'none', '--dev'], { env: buildEnv });
      if (mine !== run) return;
      if (frame) {
        if (inputs.read(frame, schedule, stage, started, out.code === 0)) again = true;
        if (again || timer) return;
      }
      if (out.code !== 0) { rmSync(stage, { recursive: true, force: true }); return fail(out.output.split('\n').filter((l) => l.trim() && !/^\s*(Compiling|Finished|Running|warning)/.test(l)).slice(-20).join('\n') || `${shown} build exited ${out.code}`); }
      // Every public byte a frame can display, including same-name assets. A map
      // can move after an unrelated edit without changing the page already open.
      const next = frame && JSON.stringify(buildFileCards(stage).filter(f => f.name !== 'app.plan.map.json'));
      replaceBuild(stage, dist);
      keepMap();
      failed = null;
      if (frame && built && fingerprint === next) { if (previousError) push({ error: null }); return; }
      fingerprint = next;
      seq += 1; built = true;
      log(`${shown}: built in ${Date.now() - started} ms · ${clients.size} page${clients.size === 1 ? '' : 's'}`);
      push({ reload: seq, revision: null, ...(previousError ? { error: null } : {}) });
    } catch (error) {
      if (mine === run) fail(`${label}: ${error.message}`);
    } finally {
      busy = false;
      // Asked again meanwhile (an edit, or a page after a stop): once more.
      if (again) { again = false; refresh(); }
    }
  }

  /** Its plan's map, by its digest, its own last few (never the app's store). */
  function keepMap() {
    try {
      const digest = createHash('sha256').update(readFileSync(resolve(dist, 'app.plan'))).digest('hex'), bytes = readFileSync(resolve(dist, 'app.plan.map.json'));
      if (JSON.parse(bytes).digest !== digest) return;
      maps.delete(digest); maps.set(digest, bytes);
      for (const old of [...maps.keys()].slice(0, Math.max(0, maps.size - MAPS))) maps.delete(old);
    } catch { /* no map: a selection on it is read-only */ }
  }

  /** Its quiet-period stop, while no page of it is open. */
  function quiet() {
    clearTimeout(idle); idle = null;
    if (clients.size) return;
    idle = setTimeout(() => { if (!clients.size) { log(`${shown}: no page for a while; it stopped`); active = false; const stopped = stop(); onIdle?.(stopped); } }, idleMs);
  }

  /** Asked for by a page: made, if it has no build or was stopped since, or failed; its quiet period begun again.
   * A page joining the build under way (its first page's stream) adds none. */
  function ensure() {
    const wasActive = active;
    active = true;
    if (frame && !wasActive) inputs.begin(frame, schedule);
    if (busy) { if (!wasActive || failed) again = true; } else if (!built || !wasActive || failed) refresh();
    quiet();
  }

  /** Its build stopped; started again by the next page. */
  function stop() {
    clearTimeout(timer); timer = null;
    active = false;
    inputs?.forget(schedule);
    clearTimeout(idle); idle = null; run += 1; again = false; tables.clear();
    return Promise.all([...procs].map((c) => {
      const gone = new Promise((ok) => { if (c.exitCode !== null || c.signalCode !== null) ok(); c.once('exit', ok); setTimeout(ok, 5000); });
      try { process.kill(-c.pid, 'SIGKILL'); } catch { try { c.kill('SIGKILL'); } catch { /* gone */ } }
      return gone;
    }));
  }

  /** Its plan's inspect table (the loop's own command, a process of the gallery's), the last two. */
  function inspectTable(digest) {
    if (!tables.has(digest)) {
      const tool = tableTool(), out = resolve(cache, `inspect-${digest}-${process.pid}.json`); // (this loop's own, as dev.mjs's)
      tables.set(digest, (async () => {
        if (!tool) return null;
        const made = await step(tool.cmd, [...tool.pre, 'inspect-table', resolve(dist, 'app.plan'), out], tool.env ? { env: tool.env } : {});
        tool.done?.();
        let bytes = null;
        try { bytes = made.code === 0 ? readFileSync(out) : null; if (bytes && JSON.parse(bytes).plan !== digest) bytes = null; } catch { bytes = null; }
        rmSync(out, { force: true });
        if (!bytes) tables.delete(digest);
        return bytes;
      })());
      for (const old of [...tables.keys()].slice(0, Math.max(0, tables.size - 2))) tables.delete(old);
    }
    return tables.get(digest);
  }

  /** The stream: the build channel's (its commands, its nonce), the gallery's reloads and errors. */
  function stream(req, res) {
    res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-store', connection: 'keep-alive' });
    res.write(':\n\n');
    const opened = channel()?.open((frame) => res.write(json(frame))) ?? null;
    res.write(json({ ...(failed ? { error: failed } : {}), reload: seq, revision: null, ...(opened?.nonce ? { nonce: opened.nonce } : {}) }));
    clients.add(res);
    quiet();
    // Ended once, by whichever close comes first (under Bun a request on a handed-off socket never fires req 'close').
    let done = false;
    const end = () => { if (done) return; done = true; clients.delete(res); opened?.close(); if (active) quiet(); };
    req.on('close', end);
    res.on('close', end);
  }

  const send = (req, res, found, body) => sendStaticBody(req, res, body ?? readFileSync(found.path), { 'content-type': webContentType(found.route), 'cache-control': 'no-store' });

  return {
    cache,
    /** Whether the request was the gallery's, answered. */
    async handle(req, res, url) {
      if (url.pathname !== base && !url.pathname.startsWith(base + '/')) return false;
      const rest = url.pathname.slice(base.length) || '/';
      if (url.pathname === controls + '/stream') { ensure(); stream(req, res); return true; }
      // No live design: the gallery is built again for a kept edit (a page asking for the design hears none).
      if (url.pathname === controls + '/design' || url.pathname === controls + '/reloaded') { res.writeHead(204, { 'cache-control': 'no-store' }); res.end(); return true; }
      // Nothing of a gallery page is carried across its reloads: it loads afresh.
      if (url.pathname === controls + '/checkpoint') { res.writeHead(403, { 'cache-control': 'no-store' }); res.end(); return true; }
      // Its inspect table, of its own plan.
      if (url.pathname === controls + '/dev-inspect.json' && req.method === 'GET') {
        quiet(); // (its table's process stopped with the rest when no page of it is open a while)
        let digest = null;
        try { digest = createHash('sha256').update(readFileSync(resolve(dist, 'app.plan'))).digest('hex'); } catch { /* no build */ }
        const bytes = digest && await inspectTable(digest);
        if (!bytes) { res.writeHead(503, { 'cache-control': 'no-store' }); res.end(); return true; }
        res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
        res.end(bytes);
        return true;
      }
      ensure();
      // Its assets are served from the app's dist.
      if (!frame && rest.startsWith('/assets/')) {
        const found = await buildTreeFile(appDist, rest, req.headers.accept);
        if (!found) { res.writeHead(404, { 'cache-control': 'no-store' }); res.end(); return true; }
        send(req, res, found);
        return true;
      }
      const found = built ? await buildTreeFile(dist, rest, req.headers.accept) : null;
      if (!found) {
        if (rest !== '/' && !(frame && !built && req.headers.accept?.includes('text/html'))) { res.writeHead(404, { 'cache-control': 'no-store' }); res.end(); return true; }
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
        res.end(waiting(label, controls + '/stream', failed));
        return true;
      }
      let body = null;
      if (found.route.endsWith('.html')) {
        // Its files are under the gallery's path (a page with no `<base>` given one); its dev client the gallery's.
        body = readFileSync(found.path).toString();
        const mount = `<base href="${base}/">`;
        if (!frame) body = body.includes('<base href="/">') ? body.replace('<base href="/">', mount) : /<head[^>]*>/.test(body) ? body.replace(/<head[^>]*>/, (h) => `${h}${mount}`) : body.replace(/^(<!doctype[^>]*>)?/i, (d) => `${d}${mount}`);
        body += client(seq);
      }
      send(req, res, found, body);
      return true;
    },
    /** The app published a reload: the gallery made and built again, if a page asked for it since its last stop. */
    appChanged() { if (active) refresh(); },
    schedule,
    /** The app's committed design is `digest` (a kept edit or a scrub's end, never a scrub's frame): built again
     * when it moved. */
    designKept(digest) { if (digest === design) return; design = digest; if (active) refresh(); },
    /** Its plan's map by digest, if it is one of its own. */
    map: (digest) => maps.get(digest) ?? null,
    stop,
    /** For tests and the timing line. */
    state: () => ({ seq, built, active, busy, pages: clients.size, failed, cache }),
  };
}

/** Subscribe by source name so a shared package has one directory watch across all open frames. */
function frameInputs() {
  const owners = new Map(), directories = new Map();
  function subscribe(notify, paths) {
    const before = owners.get(notify) ?? new Set(), moved = new Set();
    for (const path of new Set([...before, ...paths])) {
      if (before.has(path) === paths.has(path)) continue;
      const dir = dirname(path), name = basename(path);
      if (!directories.has(dir)) directories.set(dir, { files: new Map() });
      const { files } = directories.get(dir);
      if (!files.has(name)) { files.set(name, new Set()); moved.add(dir); }
      const clients = files.get(name);
      if (paths.has(path)) clients.add(notify); else clients.delete(notify);
      if (!clients.size) { files.delete(name); moved.add(dir); }
    }
    for (const dir of moved) {
      const entry = directories.get(dir);
      if (!entry.files.size) { entry.handle?.close(); directories.delete(dir); continue; }
      if (entry.handle) { entry.handle.updateNames(); continue; }
      entry.handle = watchDirectoryInputs(dir, name => {
        const targets = name ? entry.files.get(name) ?? [] : new Set([...entry.files.values()].flatMap(set => [...set]));
        for (const target of targets) target();
      }, { readNames: () => entry.files.keys() });
    }
    if (paths.size) owners.set(notify, paths); else owners.delete(notify);
    return before;
  }
  return {
    begin(frame, notify) { subscribe(notify, new Set([frame, frameFixturePath(frame)])); },
    read(frame, notify, stage, began, succeeded) {
      let graph;
      try { graph = JSON.parse(readFileSync(resolve(stage, '.gen/dev-sources.json'), 'utf8')); } catch { graph = {}; }
      const contractPaths = [frame, frameFixturePath(frame), ...(graph.sources ?? []), ...(graph.shallow ?? []).map(([dir, name]) => resolve(dir, name))].map(p => resolve(p));
      const paths = new Set(contractPaths);
      const { sourceInputs } = builtSettings(stage);
      // The checked capture names imported helpers, package code and type declarations.
      // Its resolver also records missing candidates that could take precedence after an edit.
      // Staging directories sit on the checker's lookup path and move when the build is published.
      const staged = sourceInputs && realpathSync(stage);
      const reads = (sourceInputs?.reads ?? []).filter(input => !within(staged, input.path));
      for (const input of reads) paths.add(input.path);
      if (!succeeded) for (const p of owners.get(notify) ?? []) paths.add(p);
      const before = subscribe(notify, paths);
      if (succeeded && (changedContractReads(contractReadReceipt(resolve(stage, '.gen'))).length || sourceInputChanges({ reads }).length)) return true;
      // A newly discovered source could already have moved since the compiler read it.
      for (const path of contractPaths) if (!before.has(path)) {
        const stat = statSync(path, { throwIfNoEntry: false });
        if (stat?.isFile() && stat.mtimeMs >= began) return true;
      }
      return false;
    },
    forget: notify => subscribe(notify, new Set()),
  };
}

/** Each file in frames/ runs on its own stream, after the first request for it. */
export function frameLoops(options) {
  const loops = new Map(), inputs = frameInputs(), root = realpathSync(options.app.dir);
  let folder;
  return {
    async handle(req, res, url) {
      if (!url.pathname.startsWith('/__dev/frames/')) return false;
      const missing = () => { res.writeHead(404, { 'cache-control': 'no-store' }); res.end(); return true; };
      const part = url.pathname.slice('/__dev/frames/'.length).split('/')[0];
      let name;
      try { name = decodeURIComponent(part); } catch { return missing(); }
      if (!name || name.startsWith('.') || /[\/\\\x00-\x1f]/.test(name)) return missing();
      const base = frameURL(name), entry = resolve(root, 'frames', name + '.contract');
      if (!within(root, entry)) return missing();
      if (base !== '/__dev/frames/' + part) {
        res.writeHead(307, { location: base + url.pathname.slice('/__dev/frames/'.length + part.length) + url.search, 'cache-control': 'no-store' });
        res.end(); return true;
      }
      if (!loops.has(name)) {
        let path;
        try { path = realpathSync(entry); } catch { return missing(); }
        if (!within(root, path) || !statSync(path, { throwIfNoEntry: false })?.isFile()) return missing();
        folder ??= mkdtempSync(resolve(tmpdir(), 'exact-dev-frames-'));
        const cache = mkdtempSync(resolve(folder, 'frame-'));
        const loop = previewLoop({ ...options, frame: entry, base, label: name, cache, inputs,
          client: seq => options.client(seq, base + '/__dev'),
          onIdle: stopped => {
            loops.delete(name);
            void stopped.then(() => rmSync(cache, { recursive: true, force: true }));
          } });
        loops.set(name, loop);
      }
      return loops.get(name).handle(req, res, url);
    },
    changed() { for (const loop of loops.values()) loop.schedule(); },
    async stop() {
      await Promise.all([...loops.values()].map(loop => loop.stop()));
      loops.clear();
      if (folder) rmSync(folder, { recursive: true, force: true });
    },
  };
}
