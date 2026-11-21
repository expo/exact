// Staged capabilities (LLP 1047.000 §9): the production web artifact is split
// into a core and one module per staged capability, which the glue loads
// before the capability's first use.
//
// A stage holds what its seam entries reach and the core does not, on
// binaryen's call graph. The core's roots are its exports and address-taken
// functions, less the seam entries, so a helper the core shares stays in the
// core whatever its name. A direct call from the core into a stage is a build
// failure; the core reaches a stage only through its seam, an address-taken
// function the glue gates. The modules share the core's memory and tables
// (`wasm-split --multi-split --no-placeholders`), so a stage's functions work
// on the runner's own state, and a call into a stage not yet loaded traps: an
// assertion that a gate was bypassed, never a fallback.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { devNull, tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { gzipSync } from 'node:zlib';
import { withLock } from './build-lock.mjs';

/** The staged capabilities and their seam entries, by symbol: Rust's v0
 * mangling, which the web nightly uses, names each instantiation of a generic
 * function `_RINv…` and nothing nested in it so. A pattern's first group is
 * the defining crate's disambiguator, which all of a seam's entries share. */
export const STAGES = [
  // Inspection (LLP 1012): the agent API's reads. Host::agent calls the hook
  // `HostLinks::of` installs; the glue's `ask` is its only caller. Only
  // `exact_runner::agent::handle::<D>` is a seam entry. A demangled prefix
  // once matched its `|_| false` closure too, which wasm-opt had merged with
  // the core's identical closures under that name, and the split moved what
  // the core calls through the table into the stage.
  { name: 'inspection', seam: /^_RINvNtC(s[0-9A-Za-z]*_)?12exact_runner5agent6handle/ },
];
/** The binaryen that splits. A stage imports the core's generated export
 * names, so the pair comes from one run of one version; another version
 * builds unsplit rather than guess. */
export const BINARYEN = 'version 132';
/** Where the pinned binaryen comes from on a machine without a package
 * manager (LLP 1054 O2): the release tarball, unpacked, its `bin/` on PATH. */
export const BINARYEN_DOWNLOAD = `https://github.com/WebAssembly/binaryen/releases/tag/${BINARYEN.replace(' ', '_')}`;
const FEATURES = ['--enable-bulk-memory', '--enable-nontrapping-float-to-int', '--enable-sign-ext', '--enable-mutable-globals', '--enable-reference-types'];

const run = (command, args, options = {}) => spawnSync(command, args, { encoding: 'utf8', maxBuffer: 1 << 28, ...options });

/** GPU packaging needs wasm-bindgen; wasm-opt remains optional. */
export function webGpuTools(env = process.env) {
  const ready = run('wasm-bindgen', ['--version'], { env });
  if (ready.error || ready.status !== 0) throw new Error(`wasm-bindgen unavailable; run bun scripts/exact.mjs setup in the SDK checkout: ${ready.error?.message ?? ready.stderr ?? ''}`);
}

/** Package one GPU crate as its browser module and return its size note. */
export function packageGpuWasm(crate, wasm, stage, stem, { env, profile }) {
  const [dir, name] = stem.includes('/') ? [resolve(stage, 'gpu'), stem.slice(4)] : [stage, stem];
  const wb = run('wasm-bindgen', ['--target', 'web', '--no-typescript', '--out-dir', dir, '--out-name', name, wasm], { env, stdio: 'inherit' });
  if (wb.status !== 0) throw new Error(`wasm-bindgen ${crate} failed`);
  const bg = resolve(stage, `${stem}_bg.wasm`);
  // EXACT_WEB_NAMES retains GPU function names for profiles, as it does for app.wasm.
  const o = profile === 'gpu-dev' ? { status: null } : run('wasm-opt', ['-Oz', '--enable-bulk-memory', '--enable-nontrapping-float-to-int', '--enable-sign-ext', '--enable-mutable-globals', env.EXACT_WEB_NAMES === '1' ? '-g' : '--strip-debug', '--strip-producers', '-o', bg, bg], { env, stdio: 'inherit' });
  const bytes = readFileSync(bg), kib = n => `${(n / 1024).toFixed(0)} KiB`;
  return `${stem}_bg.wasm ${kib(bytes.length)} (${kib(gzipSync(bytes, { level: 9 }).length)} gzip${o.status === 0 ? ', wasm-opt' : ''}), ${stem}.js ${kib(readFileSync(resolve(stage, `${stem}.js`)).length)}`;
}

export function normalizeDemangled(name) {
  // GNU prints unpadded crate hashes; LLVM omits them. Keep slices like [f64].
  return name.replace(/\[[0-9a-f]+\](?=::)/g, '');
}

/** Why this machine can't split, or null when it can. */
export function unsplitReason() {
  const version = run('wasm-split', ['--version']);
  if (version.error) return `wasm-split is not on PATH (binaryen ${BINARYEN}: brew install binaryen, or ${BINARYEN_DOWNLOAD})`;
  if (!version.stdout.includes(BINARYEN)) return `wasm-split is ${version.stdout.trim()}, not the pinned ${BINARYEN} (${BINARYEN_DOWNLOAD})`;
  if (run('c++filt', ['--version']).error) return 'c++filt is not on PATH';
  return null;
}

/** A module the planner cannot show safe to split, which the build ships
 * unsplit with the reason in its note. */
export class Unsplittable extends Error {}

/** Each stage's functions (raw names) in `named`, a wasm that keeps its name
 * section. Throws `Unsplittable` when it cannot show that the core reaches a
 * stage only through its seam's one table slot, which the glue gates, and an
 * `Error` when the core calls a seam directly or no function is one. */
export function planStages(named, stages = STAGES) {
  const bytes = readFileSync(named);
  // The call graph roots exports and table slots, not a start function, which runs at instantiation.
  if (hasStart(bytes)) throw new Unsplittable(`${named} has a start function, which the call graph does not root, so the planner cannot tell whether instantiation reaches a stage`);
  const shape = unprovable(named, bytes);
  if (shape) throw new Unsplittable(`${named}: ${shape}, so the call graph may not hold every call into a stage`);
  // A `call_indirect` of a constant slot is a call the core makes as surely as a direct one: `--directize` makes it
  // one, so the direct-call check below sees it, taking each slot's first function as its function, which
  // `unprovable` has shown nothing changes but a stage's own instantiation, filling its own slots. A slot read at run
  // time, as the hook `HostLinks::of` installs, stays the gate's to guard.
  const graph = run('wasm-opt', [named, '--directize', '--pass-arg=directize-initial-contents-immutable', '--print-call-graph', ...FEATURES, '-o', devNull]);
  if (graph.status !== 0) throw new Error(`wasm-opt --print-call-graph failed: ${graph.stderr}`);
  const nodes = new Map(), calls = new Map(), callers = new Map();
  // The graph's key is a subgraph of example nodes, which a function may share a name with ("B").
  let key = false;
  for (const line of graph.stdout.split('\n')) {
    if (/^\s*subgraph cluster_key\b/.test(line)) { key = true; continue; }
    if (key) { if (/^\s*}\s*$/.test(line)) key = false; continue; }
    const edge = /^\s*"(.+?)" -> "(.+?)";?\s*(\/\/.*)?$/.exec(line);
    if (edge) {
      if (!calls.has(edge[1])) calls.set(edge[1], new Set());
      if (!callers.has(edge[2])) callers.set(edge[2], new Set());
      calls.get(edge[1]).add(edge[2]); callers.get(edge[2]).add(edge[1]);
      continue;
    }
    const node = /^\s*"(.+?)" \[(.*)\];?\s*$/.exec(line);
    // An export is gray, and each table slot a function fills is a rounded line.
    if (node) {
      const seen = nodes.get(node[1]) ?? { exported: false, slots: 0 };
      if (/fillcolor="gray"/.test(node[2])) seen.exported = true;
      if (/rounded/.test(node[2])) seen.slots++;
      nodes.set(node[1], { ...seen, root: seen.exported || seen.slots > 0 });
    }
  }
  for (const name of [...calls.keys(), ...callers.keys()]) if (!nodes.has(name)) nodes.set(name, { exported: false, slots: 0, root: false });
  const raw = [...nodes.keys()];
  const plain = run('c++filt', [], { input: raw.join('\n') }).stdout.split('\n');
  const demangled = new Map(raw.map((name, i) => [name, normalizeDemangled(plain[i] ?? name)]));
  const entries = new Map(stages.map(stage => [stage.name, raw.filter(name => stage.seam.test(name))]));
  const seams = new Set([...entries.values()].flat());
  const reach = (starts, blocked) => {
    const seen = new Set(), stack = [...starts];
    while (stack.length) {
      const f = stack.pop();
      if (seen.has(f) || blocked.has(f)) continue;
      seen.add(f);
      for (const g of calls.get(f) ?? []) stack.push(g);
    }
    return seen;
  };
  const core = reach(raw.filter(name => nodes.get(name).root && !seams.has(name)), seams);
  const planned = [];
  for (const stage of stages) {
    const seam = entries.get(stage.name);
    if (!seam.length) throw new Error(`stage ${stage.name}: no function matches its seam ${stage.seam}`);
    const crates = new Set(seam.map(entry => stage.seam.exec(entry)[1]));
    if (crates.size > 1) throw new Unsplittable(`stage ${stage.name}: its seam ${stage.seam} matches functions of ${crates.size} crates, so another crate shares the SDK's path; build it unsplit`);
    for (const entry of seam) {
      // The glue gates a seam's one table slot. An export, or another slot (a function wasm-opt merged with the seam
      // once filled its own), is a way the core may reach the stage before it loads, and would find it empty.
      const { exported, slots } = nodes.get(entry);
      if (!exported && !slots) throw new Error(`stage ${stage.name}: ${demangled.get(entry)} is not address-taken, so it is no seam`);
      if (exported || slots !== 1) throw new Unsplittable(`stage ${stage.name}: its seam ${demangled.get(entry)} is ${exported ? 'exported' : `in ${slots} table slots`}, so the core may reach it other than through the gated slot, which a split leaves empty until the stage loads; give the seam a body no other function shares, or build unsplit`);
      const direct = [...callers.get(entry) ?? []].filter(caller => core.has(caller));
      if (direct.length) throw new Error(`stage ${stage.name}: the core calls its seam ${demangled.get(entry)} directly from ${direct.slice(0, 3).map(c => demangled.get(c)).join(', ')}`);
    }
    planned.push({ name: stage.name, functions: [...reach(seam, new Set())].filter(f => !core.has(f)) });
  }
  return planned;
}

/** Why the planner cannot read every call into a stage off the call graph of the module at `named` (in `bytes`), or
 * null: what could change a table slot from its first function (a table imported or exported, which a script may write;
 * an instruction that sets, fills, copies, grows or initializes one), a slot whose first function is not plain (an
 * element segment at an offset that is no constant, or over another's slots), or a function reference that leaves the
 * table (`ref.func` in code or a global, or `table.get`), which wasm-split turns into a trampoline the stage fills. */
function unprovable(named, bytes) {
  let at = 0;
  const leb = () => { let value = 0; for (let shift = 0, byte = 0x80; byte & 0x80; shift += 7) { byte = bytes[at++]; value += (byte & 0x7f) * 2 ** shift; } return value; };
  const sleb = () => { let value = 0, shift = 0, byte; do { byte = bytes[at++]; value += (byte & 0x7f) * 2 ** shift; shift += 7; } while (byte & 0x80); return byte & 0x40 ? value - 2 ** shift : value; };
  const name = () => { const length = leb(); at += length; };
  // A value type: one byte, or a reference's 0x63 or 0x64 and its heap type
  const valueType = () => { const type = bytes[at++]; if (type === 0x63 || type === 0x64) sleb(); else if (type < 0x6f) return false; return true; };
  // A constant offset: `i32.const n` and `end`
  const offset = () => { if (bytes[at++] !== 0x41) return null; const value = sleb(); return bytes[at++] === 0x0b ? value : null; };
  const slots = [];
  for (at = 8; at < bytes.length;) {
    const id = bytes[at++], size = leb(), end = at + size;
    if (id === 2) {
      for (let count = leb(); count > 0; count--) {
        name(); name();
        const kind = bytes[at++];
        if (kind === 0) leb();
        else if (kind === 1) return 'it imports a table';
        else if (kind === 2) { const flags = bytes[at++]; if (flags & ~0xf) return 'it imports a memory the planner cannot read'; leb(); if (flags & 1) leb(); if (flags & 8) leb(); }
        else if (kind === 3) { if (!valueType()) return 'it imports a global the planner cannot read'; at++; }
        else if (kind === 4) { at++; leb(); }
        else return 'it imports what the planner cannot read';
      }
      if (at !== end) return 'its imports are not as the planner reads them';
    } else if (id === 7) {
      for (let count = leb(); count > 0; count--) { name(); if (bytes[at++] === 1) return 'it exports a table'; leb(); }
    } else if (id === 9) {
      for (let count = leb(); count > 0; count--) {
        const flags = leb();
        if (flags > 7) return 'it has an element segment the planner cannot read';
        const active = !(flags & 1), table = flags & 2 && active ? leb() : 0;
        const start = active ? offset() : 0;
        if (start === null) return 'an element segment starts at an offset that is no constant';
        if (flags & 2 || !active) at++;
        const items = leb();
        for (let item = 0; item < items; item++) {
          if (!(flags & 4)) { leb(); continue; }
          const op = bytes[at++];
          if (op === 0xd2) leb(); else if (op === 0xd0) at++; else return 'an element segment holds what the planner cannot read';
          if (bytes[at++] !== 0x0b) return 'an element segment holds what the planner cannot read';
        }
        if (!active) continue;
        if (table !== 0) return 'an element segment fills a second table';
        if (slots.some(([from, to]) => start < to && from < start + items)) return 'two element segments fill one slot';
        slots.push([start, start + items]);
      }
    }
    at = end;
  }
  const metrics = run('wasm-opt', [named, '--metrics', ...FEATURES, '-o', devNull]);
  if (metrics.status !== 0) throw new Error(`wasm-opt --metrics failed: ${metrics.stderr}`);
  const count = (key) => Number(new RegExp(`^\\s*${key.replace(/[[\]]/g, '\\$&')}\\s*:\\s*(\\d+)`, 'm').exec(metrics.stdout)?.[1] ?? 0);
  const writes = ['TableSet', 'TableFill', 'TableCopy', 'TableGrow', 'TableInit', 'TableGet'].filter((key) => count(key) > 0);
  if (writes.length) return `its code holds ${writes.join(', ')}`;
  // Binaryen counts each element segment's entry as a `ref.func`; more are references in code or a global
  if (count('RefFunc') > count('[table-data]')) return 'a function reference leaves the table, through code or a global';
  return null;
}

/** Whether the module in `bytes` has a start section (id 8). */
function hasStart(bytes) {
  for (let at = 8; at < bytes.length;) {
    const id = bytes[at++];
    let size = 0;
    for (let shift = 0, byte = 0x80; byte & 0x80; shift += 7) { byte = bytes[at++]; size += (byte & 0x7f) * 2 ** shift; }
    if (id === 8) return true;
    at += size;
  }
  return false;
}

/** `from` without its names and producers, written at `to`; its bytes. */
export function stripWasm(from, to) {
  const opt = run('wasm-opt', [from, '--strip-debug', '--strip-producers', ...FEATURES, '-o', to]);
  if (opt.status !== 0) throw new Error(`wasm-opt --strip-debug failed: ${opt.stderr}`);
  return readFileSync(to);
}

/** A wasm custom section: id 0, its size, then its name and payload. */
function customSection(name, payload) {
  const leb = (n) => { const out = []; do { let b = n & 0x7f; n >>>= 7; if (n) b |= 0x80; out.push(b); } while (n); return Buffer.from(out); };
  const body = Buffer.concat([leb(Buffer.byteLength(name)), Buffer.from(name), payload]);
  return Buffer.concat([Buffer.from([0]), leb(body.length), body]);
}

/** Split `named` (optimized, names kept) into a core and its stages. Returns
 * the core's bytes and each stage's `stages/<name>.<digest>.wasm`; the core's
 * `exact.stages` custom section names them, so a core never loads another
 * build's stage (the digest is the stage's content). */
export function splitStages(named, stages = STAGES) {
  const planned = planStages(named, stages).filter(stage => stage.functions.length);
  const work = mkdtempSync(resolve(tmpdir(), 'exact-stages-'));
  try {
    const manifest = resolve(work, 'manifest.txt');
    writeFileSync(manifest, planned.map(stage => [stage.name, ...stage.functions].join('\n')).join('\n\n') + '\n');
    const split = run('wasm-split', ['--multi-split', named, '--manifest', manifest, '--out-prefix', resolve(work, 'stage-'), '-o', resolve(work, 'core.wasm'), '--no-placeholders', ...FEATURES]);
    if (split.status !== 0) throw new Error(`wasm-split failed: ${split.stderr}`);
    const strip = (from) => stripWasm(from, from.replace(/\.wasm$/, '.stripped.wasm'));
    const files = new Map(), table = {};
    for (const stage of planned) {
      const bytes = strip(resolve(work, `stage-${stage.name}.wasm`));
      const path = `stages/${stage.name}.${createHash('sha256').update(bytes).digest('hex').slice(0, 16)}.wasm`;
      files.set(path, bytes);
      table[stage.name] = path;
    }
    const core = Buffer.concat([strip(resolve(work, 'core.wasm')), customSection('exact.stages', Buffer.from(JSON.stringify({ binaryen: BINARYEN, stages: table })))]);
    return { core, files, report: planned.map(stage => `${stage.name} ${stage.functions.length} functions, ${(files.get(table[stage.name]).length / 1024).toFixed(0)} KiB`) };
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

/** Optimize the app before capability splitting, reusing only a verified product of the same tool and inputs. */
export function optimizeWasm(input, output, options, cache, tool = 'wasm-opt') {
  // Bun's default lookup retains the startup PATH; the build adds its pinned Binaryen at runtime.
  const executable = Bun.which(tool, { PATH: process.env.PATH });
  const optimize = (command, source) => spawnSync(command, [...options, '-o', output, source], { stdio: 'inherit' });
  if (!executable) return optimize(tool, input);
  const sha = bytes => createHash('sha256').update(bytes).digest('hex');
  const bytes = readFileSync(input), path = realpathSync(executable), identity = sha(readFileSync(path));
  const key = sha(JSON.stringify([sha(bytes), options, path, identity]));
  mkdirSync(cache, { recursive: true });
  return withLock(resolve(cache, 'lock'), () => {
    const product = resolve(cache, 'optimized.wasm'), receipt = resolve(cache, 'receipt.json');
    let kept = null;
    try {
      const card = JSON.parse(readFileSync(receipt, 'utf8'));
      if (card.key === key) {
        const candidate = readFileSync(product);
        if (sha(candidate) === card.sha256) kept = candidate;
      }
    } catch { /* A missing or incomplete product is rebuilt. */ }
    if (kept) { writeFileSync(output, kept); return { status: 0, cached: true }; }
    // Cargo may replace the original while Binaryen runs; optimize the bytes whose digest keys this entry.
    const snapshot = resolve(cache, 'input.wasm');
    writeFileSync(snapshot, bytes);
    let result;
    try { result = optimize(path, snapshot); }
    finally { rmSync(snapshot, { force: true }); }
    if (result.status === 0 && sha(readFileSync(path)) === identity) {
      const optimized = readFileSync(output);
      writeFileSync(product, optimized);
      writeFileSync(receipt, JSON.stringify({ key, sha256: sha(optimized) }) + '\n');
    }
    return result;
  });
}
