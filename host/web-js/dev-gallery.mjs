// On-demand pages on the JS dev server. The gallery (LLP 1139 D5) builds
// generated library examples. Frames build authored entries with the app's
// data. Each preview owns its build, error stream and open pages, and stops
// after its last page disconnects.
import { same, sameShape } from './dev-inline.mjs';
import { designPath } from '../../scripts/forma-design-path.mjs';
import { streamFrame } from '../../scripts/forma-builds.mjs';
import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve, sep } from 'node:path';
import { availableParallelism, tmpdir } from 'node:os';
import { builtSettings, sourceInputChanges } from '../../js/bake/src/__exact_env.mjs';
import { changedContractReads, contractReadReceipt } from '../../scripts/web-build.mjs';
import { filesystemRead } from '../../scripts/filesystem.mjs';
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
async function sharedFile(root, rest) {
  let path;
  try { path = decodeURIComponent(rest.slice(1)).replace(/\/$/, ''); } catch { /* invalid escape */ }
  if (path && !/%2f/i.test(rest) && path.split('/').every(part => part && !part.startsWith('.') && !/[\\:\x00-\x1f]/.test(part))) {
    for (const file of [path, path + '/index.html']) {
      try {
        const body = Buffer.from(await filesystemRead({ op: 'get', root, path: file }), 'base64');
        return { route: '/' + file, body };
      } catch { /* try the directory index, then a miss */ }
    }
  }
  return null;
}
function previewLoop({ app, root, appDist, base, label, cache, frame, inputs, queue, onIdle, onBuilt, channel = () => null, client, tableTool = () => null, compiler = () => null, design: designNow = () => null, env = process.env, log = console.log, idleMs = 60_000 }) {
  const epoch = randomBytes(12).toString('hex');
  const controls = frame ? base + '/__dev' : base;
  const source = resolve(cache, 'source'), live = resolve(cache, 'live'), stage = resolve(cache, 'stage'), dist = resolve(cache, 'dist');
  const clients = new Set(), maps = new Map(), tables = new Map(), procs = new Set();
  // `seq` only grows (a page that reconnects compares against it); `active` while a page has asked since the last
  // idle stop; `run` counts stops, so what a stopped run finishes says nothing.
  let seq = 0, built = false, active = false, failed = null, busy = false, again = false, idle = null, run = 0, design = null;

  let publicAssets = new Set(), capturedAssets = new Set(), publicDirty = false, queued = false;
  const requestedAssets = new Set();
  let generation = 0, committedSource = null, fingerprint = null, timer = null, committed = null, shownDesign = null, snapshot = null;
  const notice = () => snapshot && { kind: 'forma-batch', structure: snapshot.structure, base: snapshot.design, design: snapshot.design, tokens: [], scrub: snapshot.scrub };
  function showDesign(next, scrub) {
    const from = shownDesign;
    shownDesign = next;
    snapshot = { kind: 'forma-snapshot', structure: next.shape, base: null, design: next.digest, tokens: next.tokens, scrub };
    const tokens = next.tokens.filter((t, i) => !same(t, from.tokens[i]));
    if (!tokens.length && !scrub) return;
    const batch = JSON.stringify({ ...snapshot, kind: 'forma-batch', base: from.digest, tokens });
    const text = Buffer.byteLength(batch) > 32 * 1024 ? JSON.stringify(notice()) : batch;
    for (const res of clients) streamFrame(res, text, notice);
  }
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
  function refresh() {
    if (busy) { again = true; return; }
    if (queue) { queued = true; queue.add(build, () => clients.size > 0); }
    else void build();
  }

  async function build() {
    clearTimeout(timer); timer = null;
    queued = false; busy = true;
    const mine = run;
    // The design it builds on: one kept since is news, this one isn't. A failure before is taken up by this run.
    design = designNow();
    const previousError = failed;
    try {
      if (!frame) await generateGallery(mine);
      if (mine !== run) return;
      // The JS target's build, rendering no pages, into its own stage: nothing of the app's build is shared.
      rmSync(stage, { recursive: true, force: true });
      if (frame) log(`${shown}: building`);
      const started = Date.now(), webCompiler = env.EXACT_WEB_COMPILER || compiler();
      const entry = frame ? [app.dir, '--frame', frame, '--serve-frame'] : [live];
      const buildEnv = { ...env };
      if (frame) buildEnv.BUN_RUNTIME_TRANSPILER_CACHE_PATH = '0';
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
      // A map can move without changing the page already open.
      const next = frame && JSON.stringify(buildFileCards(stage).filter(f => f.name !== 'app.plan.map.json'));
      replaceBuild(stage, dist);
      keepMap();
      failed = null;
      if (frame) {
        ++generation;
        const graph = JSON.parse(readFileSync(resolve(dist, '.gen/dev-sources.json')));
        publicAssets = new Set(graph.publicAssets);
        capturedAssets = new Set(graph.captured);
        const wasCommitted = shownDesign === committed;
        try {
          committed = JSON.parse(readFileSync(resolve(dist, '.gen/dev-design.json')));
          const path = designPath(app.dir), receipt = contractReadReceipt(resolve(dist, '.gen'));
          committedSource = path && (receipt[path] ?? receipt[realpathSync(path)] ?? null);
        } catch { committed = committedSource = null; }
        if (!built || fingerprint !== next || wasCommitted) { shownDesign = committed; snapshot = null; }
        onBuilt?.();
      }
      if (frame && built && fingerprint === next && !publicDirty) { if (previousError) push({ error: null }); log(`${shown}: kept its build`); return; }
      publicDirty = false; fingerprint = next;
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
    queue?.remove(build); queued = publicDirty = false;
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
    const opened = channel()?.open((frame) => res.write(json(frame)), { map: digest => maps.get(digest), sequence: () => seq,
      ...(frame ? { design: () => shownDesign === committed && committed ? { digest: committed.digest, source: committedSource } : null } : {}) }) ?? null;
    res.write(`id: ${epoch}\n` + json({ ...(failed ? { error: failed } : {}), reload: seq, revision: null, ...(opened?.nonce ? { nonce: opened.nonce } : {}) }));
    if (snapshot) res.write(json(notice()));
    clients.add(res);
    if (frame && built && clients.size === 1) onBuilt?.();
    quiet();
    // Ended once, by whichever close comes first (under Bun a request on a handed-off socket never fires req 'close').
    let done = false;
    const end = () => { if (done) return; done = true; clients.delete(res); opened?.close(); if (active) quiet(); };
    req.on('close', end);
    res.on('close', end);
  }

  const send = (req, res, found, body) => sendStaticBody(req, res, body ?? readFileSync(found.path), { 'content-type': webContentType(found.route), 'cache-control': 'no-store' });

  async function publicFile(req, res, rest) {
    const captured = built && await buildTreeFile(dist, rest, '');
    if (captured) { send(req, res, captured); return true; }
    const found = await sharedFile(app.dir, rest);
    if (found) {
      requestedAssets.add(found.route.slice(1));
      send(req, res, found, found.route.endsWith('.html') ? found.body.toString() + client(seq, epoch, false) : found.body);
      return true;
    }
    res.writeHead(404, { 'cache-control': 'no-store' }); res.end(); return true;
  }

  return {
    cache, publicFile, observed: name => requestedAssets.add(name),
    reconnect() { for (const res of clients) res.end(); },
    /** Whether the request was the gallery's, answered. */
    async handle(req, res, url) {
      if (url.pathname !== base && !url.pathname.startsWith(base + '/')) return false;
      const rest = url.pathname.slice(base.length) || '/';
      if (url.pathname === controls + '/stream') { ensure(); stream(req, res); return true; }
      if (frame && url.pathname === controls + '/design' && snapshot) {
        res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
        res.end(JSON.stringify(snapshot)); return true;
      }
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
      if (frame && /^\/(assets|deck)\//.test(rest)) return publicFile(req, res, rest);
      // Gallery assets come from the app's published build.
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
        body += client(seq, epoch);
      }
      send(req, res, found, body);
      return true;
    },
    /** The app published a reload: the gallery made and built again, if a page asked for it since its last stop. */
    appChanged() { if (active) refresh(); },
    schedule, fail,
    publicChanged(names) {
      if (!active || !built || !names.some(name => !capturedAssets.has(name) && (publicAssets.has(name) || requestedAssets.has(name)))) return;
      if (busy || timer || queued) publicDirty = true;
      else push({ reload: ++seq, revision: null });
    },
    designRead() {
      if (!committed || !active || failed) return null;
      return { generation, entry: frame, plan: resolve(dist, 'app.plan'), base: committed.tokens.map(t => [t.name, t.slices.length ? t.slices : [t.value]]) };
    },
    accepts: next => !!sameShape(committed, next),
    publishDesign(next, mark, source, show) {
      if (source) { committed = next; committedSource = source; }
      if (show) showDesign(next, mark);
    },
    shown: () => shownDesign,
    generation: () => generation,
    restore(mark) { if (committed && shownDesign) showDesign(committed, mark); },
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
function frameInputs(appDir) {
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
    knows: path => directories.get(dirname(path))?.files.has(basename(path)),
    begin(frame, notify) { subscribe(notify, new Set([frame, frameFixturePath(frame), resolve(appDir, 'app.json')])); },
    read(frame, notify, stage, began, succeeded) {
      let graph;
      try { graph = JSON.parse(readFileSync(resolve(stage, '.gen/dev-sources.json'), 'utf8')); } catch { graph = {}; }
      const receipt = succeeded || existsSync(resolve(stage, '.gen/app.plan.map.json')) ? contractReadReceipt(resolve(stage, '.gen')) : {};
      const contractPaths = [frame, frameFixturePath(frame), resolve(appDir, 'app.json'), ...Object.keys(receipt), ...(graph.baked ?? []), ...(graph.sources ?? []), ...(graph.shallow ?? []).map(([dir, name]) => resolve(dir, name))].map(p => resolve(p));
      // The app watcher sends its design through the frame design reader before scheduling a build.
      const design = designPath(appDir);
      const designs = new Set(design ? [design, realpathSync(design)] : []);
      const paths = new Set(contractPaths.filter(path => !designs.has(path)));
      const { sourceInputs } = builtSettings(stage);
      // The checked capture names imported helpers, package code and type declarations.
      // Its resolver also records missing candidates that could take precedence after an edit.
      // Staging directories sit on the checker's lookup path and move when the build is published.
      const staged = sourceInputs && realpathSync(stage);
      const reads = (sourceInputs?.reads ?? []).filter(input => !within(staged, input.path));
      const packages = resolve(stage, '.gen/typescript/__exact_packages.json');
      if (existsSync(packages)) for (const [, , path, digest] of JSON.parse(readFileSync(packages)).files) reads.push({ kind: 'file', path, digest });
      for (const input of reads) if (!designs.has(input.path)) paths.add(input.path);
      if (!succeeded) for (const p of owners.get(notify) ?? []) paths.add(p);
      const before = subscribe(notify, paths);
      if (succeeded && (changedContractReads(receipt).length || sourceInputChanges({ reads }).length)) return true;
      // A newly discovered source could already have moved since the compiler read it.
      for (const path of contractPaths) if (!before.has(path)) {
        const stat = statSync(path, { throwIfNoEntry: false });
        if ((stat?.isFile() && stat.ctimeMs >= began) || (!stat && succeeded && graph.baked?.includes(path))) return true;
      }
      return false;
    },
    forget: notify => subscribe(notify, new Set()),
  };
}

/** A waiting frame has one place regardless of how many edits it sees. Check
 * for an open stream at dispatch, since pages can join or leave while waiting. */
export function frameBuildQueue(limit = Math.max(1, Math.floor(availableParallelism() / 2))) {
  const waiting = new Map();
  let running = 0, overtakes = 0;
  function drain() {
    while (running < limit && waiting.size) {
      const oldest = waiting.keys().next().value;
      const priority = overtakes < limit ? [...waiting].find(([, open]) => open()) : null;
      const build = priority?.[0] ?? oldest;
      overtakes = build === oldest ? 0 : overtakes + 1;
      waiting.delete(build);
      ++running;
      void build().finally(() => { --running; drain(); });
    }
  }
  return {
    add(build, open) { waiting.set(build, open); drain(); },
    remove(build) { waiting.delete(build); },
  };
}

/** Each file in frames/ runs on its own stream, after the first request for it. */
export function frameLoops(options) {
  const loops = new Map(), queue = frameBuildQueue(), inputs = frameInputs(options.app.dir), root = realpathSync(options.app.dir);
  const identity = name => {
    const info = statSync(resolve(root, name), { throwIfNoEntry: false });
    return info?.isFile() ? [info.dev, info.ino, info.size, info.ctimeMs].join('/') : null;
  };
  const publicFiles = new Map();
  for (const { name } of options.files ?? []) if (/^(assets|deck)\//.test(name)) {
    const id = identity(name);
    if (id) publicFiles.set(name, id);
  }
  let entries;
  try { entries = readdirSync(resolve(root, 'frames')); } catch { entries = []; }
  const frameSources = new Set(entries.filter(name => /(?:\.contract|\.fixture\.mts)$/.test(name)).map(name => resolve(root, 'frames', name)));
  let commitRevision = 0, previewRevision = 0, preview = null, pendingPreview = null, released = null, lastText = null;
  async function read(request, entries = [...loops.values()], connected = false) {
    const eligible = loop => connected ? loop.state().pages > 0 : loop.state().active;
    const targets = entries.filter(eligible).map(loop => [loop, loop.designRead()]).filter(([, input]) => input);
    if (!targets.length) return [];
    const { designs } = await options.readDesigns({ ...request, frames: targets.map(([, input]) => input) });
    return targets.filter(([loop, input], i) => {
      input.next = designs?.[i];
      return loop.generation() === input.generation && eligible(loop);
    });
  }
  async function showPreview(request, mark, current, entries) {
    const rows = await read(request, entries, true);
    if (!current()) return { error: 'the design changed while the frame previews were read' };
    if (rows.some(([loop, input]) => !loop.accepts(input.next))) {
      return request.patch ? { fallback: true } : { error: 'the edit changes a frame’s structure and requires a rebuild' };
    }
    for (const [loop, input] of rows) loop.publishDesign(input.next, mark, null, true);
    return {};
  }
  async function replay(loop) {
    await pendingPreview;
    const p = preview;
    if (!p || !loop.state().pages) return;
    const current = () => preview === p && !pendingPreview;
    try {
      const result = await showPreview(p.request, p.mark, current, [loop]);
      if (current() && (result.error || result.fallback)) loop.fail(result.error ?? 'the frame cannot apply this scrub');
    } catch (error) { if (current()) loop.fail(error.message); }
  }
  let folder;
  return {
    async handle(req, res, url) {
      if (!url.pathname.startsWith('/__dev/frames/')) {
        if (/^\/(assets|deck)\//.test(url.pathname) && req.headers.referer) {
          try {
            const ref = new URL(req.headers.referer);
            const part = ref.pathname.match(/^\/__dev\/frames\/([^/]+)/);
            const loop = ref.host === req.headers.host && part && loops.get(decodeURIComponent(part[1]));
            if (loop) return loop.publicFile(req, res, url.pathname);
          } catch { /* not a frame referrer */ }
        }
        return false;
      }
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
        const loop = previewLoop({ ...options, frame: entry,
          onBuilt: () => { if (preview || pendingPreview) void replay(loop); }, base, label: name, cache, inputs, queue,
          client: (seq, epoch, numbered = true) => options.client(seq, base + '/__dev', epoch, numbered),
          onIdle: stopped => {
            loops.delete(name);
            void stopped.then(() => rmSync(cache, { recursive: true, force: true }));
          } });
        loops.set(name, loop);
      }
      return loops.get(name).handle(req, res, url);
    },
    async unattributedFile(req, pathname) {
      if (req.headers.referer || !loops.size || !/^\/(assets|deck)\//.test(pathname)) return null;
      let name;
      try { name = decodeURIComponent(pathname.slice(1)); } catch { return null; }
      if (!publicFiles.has(name) && !publicFiles.has(name.replace(/\/?$/, '/index.html'))) return null;
      // An absolute URL without a referrer has the app's package overlay. The request could belong to any open frame.
      const stage = resolve(options.appDist, '.gen/typescript');
      if (existsSync(resolve(stage, '__exact_packages.json'))) {
        const { capturedAssets } = await import('../../js/bake/src/typescript.mjs');
        if (capturedAssets(stage).some(asset => name === 'assets/' + asset.name + '/' + asset.path)) return null;
      }
      const found = await sharedFile(root, pathname);
      for (const loop of loops.values()) loop.observed(found?.route.slice(1) ?? name);
      return found ?? { status: 404 };
    },
    async designEdit(text) {
      if (text === lastText && !released) return;
      lastText = text;
      const at = ++commitRevision, mark = released;
      if (mark) { ++previewRevision; preview = released = null; }
      for (const loop of loops.values()) if (!loop.designRead()) loop.schedule();
      let rows;
      try { rows = await read({ text }); }
      catch {
        if (at === commitRevision) { lastText = null; for (const loop of loops.values()) loop.schedule(); }
        return;
      }
      if (at !== commitRevision) return;
      const source = createHash('sha256').update(text).digest('hex');
      for (const [loop, input] of rows) {
        if (loop.accepts(input.next)) loop.publishDesign(input.next, mark, source, !preview);
        else loop.schedule();
      }
    },
    async preview(request, mark, current) {
      const at = ++previewRevision;
      const work = showPreview(request, mark, () => previewRevision === at && current());
      pendingPreview = work;
      try {
        const result = await work;
        if (previewRevision === at && !result.error && !result.fallback) preview = { request, mark };
        return result;
      } finally { if (pendingPreview === work) pendingPreview = null; }
    },
    async inlineWhy(text) {
      const rows = await read({ text }, undefined, true);
      const blocked = rows.find(([loop, input]) => !loop.accepts(input.next));
      return blocked ? `the design cannot be applied in place in ${basename(blocked[1].entry)}` : null;
    },
    async verify(text) {
      const at = previewRevision;
      const rows = await read({ text }, undefined, true);
      if (at !== previewRevision) return {};
      const differs = rows.find(([loop, input]) => JSON.stringify(loop.shown()) !== JSON.stringify(input.next));
      return differs ? { mismatch: `the patched frame differs from its compile: ${basename(differs[1].entry)}` } : {};
    },
    restore(mark) { ++previewRevision; preview = released = null; for (const loop of loops.values()) loop.restore(mark); },
    released(mark) { released = mark ?? true; },
    reconnect() { for (const loop of loops.values()) loop.reconnect(); },
    changed(paths) {
      let unknown = !paths;
      const publicChanges = [];
      for (const path of paths ?? []) {
        const name = relative(root, path).split(sep).join('/');
        const known = inputs.knows(path) || frameSources.has(path) || publicFiles.get(name);
        if (!known) unknown = true;
        if (/^frames\/[^/]+(?:\.contract|\.fixture\.mts)$/.test(name)) {
          if (existsSync(path)) frameSources.add(path); else frameSources.delete(path);
        }
        if (!/^(assets|deck)\//.test(name)) continue;
        const id = identity(name);
        if (publicFiles.get(name) !== id) publicChanges.push(name);
        publicFiles.set(name, id);
      }
      for (const loop of loops.values()) {
        if (unknown || loop.state().failed) loop.schedule();
        loop.publicChanged(publicChanges);
      }
    },
    async stop() {
      await Promise.all([...loops.values()].map(loop => loop.stop()));
      loops.clear();
      if (folder) rmSync(folder, { recursive: true, force: true });
    },
  };
}
