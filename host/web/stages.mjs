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
/** Binaryen release archives append their tag; package-manager builds may omit it. */
export function binaryenVersion(output) {
  return /^wasm-opt (version \d+)(?: \([^)]*\))?$/.exec(output.trim())?.[1] ?? output.trim();
}

export function binaryenArchive(version, os = process.platform, cpu = process.arch) {
  const platform = {darwin:'macos',linux:'linux',win32:'windows'}[os];
  const arch = cpu === 'x64' ? 'x86_64' : cpu === 'arm64' && os !== 'win32'
    ? (os === 'linux' ? 'aarch64' : 'arm64') : null;
  if (!platform || !arch) throw new Error(`no Binaryen setup for ${os}/${cpu}`);
  return `binaryen-${version}-${arch}-${platform}.tar.gz`;
}

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
 * section. Throws an `Error` when no function is a stage's seam or the core
 * calls one directly, which fail the build (LLP 1047.000 §9), and
 * `Unsplittable` when it cannot show that the core reaches a stage only
 * through its seam's one table slot, which the glue gates. */
export function planStages(named, stages = STAGES) {
  const bytes = readFileSync(named);
  // What fails the build whatever the module's shape comes first, off the plain call graph.
  const graph = callGraph(named, []);
  const plan = planOn(graph, stages);
  // The call graph roots exports and table slots, not a start function, which runs at instantiation.
  if (hasStart(bytes)) throw new Unsplittable(`${named} has a start function, which the call graph does not root, so the planner cannot tell whether instantiation reaches a stage`);
  const shape = unprovable(named, bytes);
  if (shape) throw new Unsplittable(`${named}: ${shape}, so a table slot may hold another function at run time than the one it starts with`);
  for (const { stage, entry } of plan.entries) {
    // The glue gates a seam's one table slot. An export, or another slot (a function wasm-opt merged with the seam
    // once filled its own), is a way the core may reach the stage before it loads, and would find it empty.
    const { exported, slots } = graph.nodes.get(entry);
    if (exported || slots !== 1) throw new Unsplittable(`stage ${stage.name}: its seam ${plan.demangled.get(entry)} is ${exported ? 'exported' : `in ${slots} table slots`}, so the core may reach it other than through the gated slot, which a split leaves empty until the stage loads`);
  }
  for (const stage of stages) {
    const crates = new Set(plan.seams.get(stage.name).map(entry => stage.seam.exec(entry)[1]));
    if (crates.size > 1) throw new Unsplittable(`stage ${stage.name}: its seam ${stage.seam} matches functions of ${crates.size} crates, so another crate shares the SDK's path`);
  }
  // A `call_indirect` of a constant slot is a call the core makes as surely as a direct one: `--directize`, taking each
  // slot's first function as its function, which `unprovable` has shown nothing but a stage's own instantiation
  // changes, makes it one, which fails the build as a direct call does. A slot read at run time, as the hook
  // `HostLinks::of` installs, stays the gate's to guard.
  return planOn(callGraph(named, ['--directize', '--pass-arg=directize-initial-contents-immutable']), stages).planned;
}

/** Binaryen's call graph of the module at `named`, run through `passes` first: each function's node (exported, the
 * table slots it fills), and its calls each way. Every line must be one the printer writes; a name it cannot write
 * whole, with a line break, ends the plan as `Unsplittable`. */
function callGraph(named, passes) {
  const graph = run('wasm-opt', [named, ...passes, '--print-call-graph', ...FEATURES, '-o', devNull]);
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
      continue;
    }
    if (!/^\s*(digraph call \{|rankdir = LR;|node \[shape=box, fontname=courier, fontsize=10\];|\})?\s*$/.test(line)) throw new Unsplittable(`${named}: a function name the call graph cannot hold whole, so it may hide a call`);
  }
  for (const name of [...calls.keys(), ...callers.keys()]) if (!nodes.has(name)) nodes.set(name, { exported: false, slots: 0, root: false });
  return { nodes, calls, callers };
}

/** Each stage's functions on `graph`: what its seam entries reach and the core does not. Throws when no function is a
 * stage's seam, an entry is not address-taken, or the core calls one directly. */
function planOn(graph, stages) {
  const { nodes, calls, callers } = graph, raw = [...nodes.keys()];
  const plain = run('c++filt', [], { input: raw.join('\n') }).stdout.split('\n');
  const demangled = new Map(raw.map((name, i) => [name, normalizeDemangled(plain[i] ?? name)]));
  const seams = new Map(stages.map(stage => [stage.name, raw.filter(name => stage.seam.test(name))]));
  const all = new Set([...seams.values()].flat());
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
  const core = reach(raw.filter(name => nodes.get(name).root && !all.has(name)), all);
  const planned = [], entries = [];
  for (const stage of stages) {
    const seam = seams.get(stage.name);
    if (!seam.length) throw new Error(`stage ${stage.name}: no function matches its seam ${stage.seam}`);
    for (const entry of seam) {
      const { exported, slots } = nodes.get(entry);
      if (!exported && !slots) throw new Error(`stage ${stage.name}: ${demangled.get(entry)} is not address-taken, so it is no seam`);
      const direct = [...callers.get(entry) ?? []].filter(caller => core.has(caller));
      if (direct.length) throw new Error(`stage ${stage.name}: the core calls its seam ${demangled.get(entry)} directly from ${direct.slice(0, 3).map(c => demangled.get(c)).join(', ')}`);
      entries.push({ stage, entry });
    }
    planned.push({ name: stage.name, functions: [...reach(seam, new Set())].filter(f => !core.has(f)) });
  }
  return { planned, entries, seams, demangled };
}

/** Why the call graph of the module at `named` (in `bytes`) may not hold every way into a stage, as the engine and
 * binaryen read the module, or null: a table it imports or exports, which a script may write; an import other than a
 * function's; an instruction that sets, fills, copies, grows, initializes or reads a table; or a function reference
 * outside the element segments, in code or a global, which wasm-split would turn into a trampoline into the stage.
 * Binaryen counts each segment entry as a `ref.func` (`[table-data]`) and a null one as a `ref.null` too, so with no
 * null the references beyond the entries are those. What a split leaves of the table is checked after it
 * (`splitChecked`). */
function unprovable(named, bytes) {
  let module;
  try { module = new WebAssembly.Module(bytes); } catch (e) { return `the engine cannot compile it (${e.message})`; }
  if (WebAssembly.Module.imports(module).some((entry) => entry.kind !== 'function')) return 'it imports a table, a memory, a global or a tag';
  if (WebAssembly.Module.exports(module).some((entry) => entry.kind === 'table')) return 'it exports a table';
  const metrics = run('wasm-opt', [named, '--metrics', ...FEATURES, '-o', devNull]);
  if (metrics.status !== 0) throw new Error(`wasm-opt --metrics failed: ${metrics.stderr}`);
  const count = (name) => Number(new RegExp(`^\\s*${name.replace(/[[\]]/g, '\\$&')}\\s*:\\s*(\\d+)`, 'm').exec(metrics.stdout)?.[1] ?? 0);
  const used = ['TableSet', 'TableFill', 'TableCopy', 'TableGrow', 'TableInit', 'TableGet'].filter((name) => count(name) > 0);
  if (used.length) return `its code holds ${used.join(', ')}`;
  if (count('RefNull')) return 'it holds a null function reference, which keeps the planner from counting the references outside its element segments';
  if (count('RefFunc') > count('[table-data]')) return 'a function reference leaves the element segments, through code or a global';
  return null;
}

/** Why the split in `work` would let the core lose a function before or as a stage loads, or null: a stage that, loaded,
 * changes a slot the core already holds (an element segment over another's). The core and its stages are instantiated
 * by the engine, with imports that throw. */
function splitChecked(work, planned) {
  const stubs = (module, given = {}) => {
    const imports = {};
    for (const entry of WebAssembly.Module.imports(module)) {
      (imports[entry.module] ??= {})[entry.name] = given[entry.module]?.[entry.name] ?? (() => { throw new Error('a stub import was called'); });
    }
    return imports;
  };
  try {
    const coreModule = new WebAssembly.Module(readFileSync(resolve(work, 'core.wasm')));
    const core = new WebAssembly.Instance(coreModule, stubs(coreModule)).exports;
    const tables = Object.values(core).filter((value) => value instanceof WebAssembly.Table);
    const before = tables.map((table) => Array.from({ length: table.length }, (_, i) => table.get(i)));
    for (const stage of planned) {
      const module = new WebAssembly.Module(readFileSync(resolve(work, `stage-${stage.name}.wasm`)));
      new WebAssembly.Instance(module, stubs(module, { primary: core }));
    }
    for (const [t, table] of tables.entries()) {
      for (let i = 0; i < before[t].length; i++) {
        if (before[t][i] !== null && table.get(i) !== before[t][i]) return `loading a stage changes slot ${i}, which the core already holds`;
      }
    }
  } catch (e) {
    return `the engine cannot instantiate the split to check it (${e.message})`;
  }
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
    const unsafe = splitChecked(work, planned);
    if (unsafe) throw new Unsplittable(`${named}: ${unsafe}`);
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
