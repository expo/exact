// Staged capabilities (LLP 1047.000 §9): the production web artifact is split
// into a core and one module per staged capability, which the glue loads
// before the capability's first use.
//
// A stage holds what its seam entries reach and the core does not, on
// binaryen's call graph. The core's roots are its exports and address-taken
// functions, less the seam entries, so a helper the core shares stays in the
// core whatever its name. The modules share the core's memory and tables
// (`wasm-split --multi-split`). The core imports a placeholder for each stage
// function it can reach through its table, which throws, naming the stage,
// until the stage loads and writes its functions over them (wasm/boot.js). So
// the planner decides which functions leave the core, and the glue's gate when
// the core calls them; a call around the gate is reported, not run.
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

/** A module the planner cannot split as its stages say, which the build ships
 * whole with the reason in its note. */
export class Unsplittable extends Error {}

/** Each stage's functions (raw names) in `named`, a wasm that keeps its name
 * section: what its seam reaches and the core does not. Throws an `Error`,
 * which fails the build, when no function matches a seam or the call graph
 * shows the core, a start function included, calling one directly; where the
 * pattern matches more than one crate's functions, calling each crate's with
 * the others' unblocked but for another stage's (LLP 1047.000 §9). Throws
 * `Unsplittable` when the call graph cannot be read, or cannot show that the
 * seam is the one way into the stage that the glue gates: the seam is exported
 * or not in exactly one table slot, its pattern matches functions of more than
 * one crate, or the module has a start function, which runs at instantiation
 * and may call through the table before any stage loads. */
export function planStages(named, stages = STAGES) {
  const { nodes, calls, callers, start } = callGraph(named), raw = [...nodes.keys()];
  const plain = run('c++filt', [], { input: raw.join('\n') }).stdout.replaceAll('\r\n', '\n').split('\n');
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
  // The start function runs at instantiation, so it is the core's whatever its name.
  const blocked = new Set([...all].filter(name => name !== start));
  const core = reach(raw.filter(name => nodes.get(name).root && !blocked.has(name)), blocked);
  const crates = new Map(stages.map(stage => [stage.name, new Set(seams.get(stage.name).map(entry => stage.seam.exec(entry)[1]))]));
  for (const stage of stages) {
    const seam = seams.get(stage.name);
    if (!seam.length) throw new Error(`stage ${stage.name}: no function matches its seam ${stage.seam}`);
    // A seam the core calls directly may be in no slot, once nothing takes its address, which would otherwise ship the
    // module whole. Where the pattern matches more than one crate's functions any may be the seam, so each crate's is
    // checked as if it were, the others' unblocked: the core calls the seam when it calls each crate's. Each stage is
    // checked on its own, every other stage's matches blocked but the start function, a root whatever its name, so a
    // direct call from a function that only some crate's reading puts in the core can ship the module whole rather
    // than fail the build.
    const direct = [], others = new Set(stages.filter(other => other !== stage).flatMap(other => seams.get(other.name)));
    for (const crate of crates.get(stage.name)) {
      const entries = seam.filter(entry => stage.seam.exec(entry)[1] === crate);
      const blocked = new Set([...all].filter(name => name !== start && (entries.includes(name) || others.has(name))));
      const reached = crates.get(stage.name).size > 1 ? reach(raw.filter(name => nodes.get(name).root && !blocked.has(name)), blocked) : core;
      const hit = entries.map(entry => ({ crate, entry, from: [...callers.get(entry) ?? []].filter(caller => reached.has(caller)) }))
        .find(({ from }) => from.length);
      if (!hit) break;
      direct.push(hit);
    }
    if (direct.length === crates.get(stage.name).size) {
      // Name each crate's candidate with its disambiguator, which the printed name drops, as two crates' print alike
      const candidate = ({ crate, entry }) => direct.length === 1 ? demangled.get(entry)
        : `${demangled.get(entry)} (${crate === undefined ? 'no crate disambiguator' : `crate disambiguator "${crate}"`})`;
      const said = direct.map(hit => `${candidate(hit)} directly from ${hit.from.slice(0, 3).map(c => demangled.get(c)).join(', ')}`);
      throw new Error(`stage ${stage.name}: the core calls its seam ${said.length > 1 ? `whichever crate's it is: ${said.join('; ')}` : said[0]}`);
    }
  }
  if (start !== null) throw new Unsplittable(`${named} has a start function, which runs at instantiation and may call through the table into a stage before it loads`);
  return stages.map((stage) => {
    const seam = seams.get(stage.name), { size } = crates.get(stage.name);
    if (size > 1) throw new Unsplittable(`stage ${stage.name}: its seam ${stage.seam} matches functions of ${size} crates, so another crate shares the SDK's path`);
    for (const entry of seam) {
      // The glue gates a seam's one table slot. An export, or another slot (a function wasm-opt merged with the seam
      // once filled its own), is a way the core may call the stage before it loads.
      const { exported, slots } = nodes.get(entry);
      if (exported || slots !== 1) throw new Unsplittable(`stage ${stage.name}: its seam ${demangled.get(entry)} is ${exported ? 'exported' : `in ${slots} table slots`}, so the core may reach it other than through the gated slot`);
    }
    return { name: stage.name, functions: [...reach(seam, new Set())].filter(f => !core.has(f)) };
  });
}

/** Binaryen's call graph of the module at `named`: each function's node (exported, the table slots it fills, the
 * start function rooted as they are), its calls each way, and the start function's name or null. */
function callGraph(named) {
  const start = startFunction(readFileSync(named));
  const graph = run('wasm-opt', [named, '--print-function-map', '--print-call-graph', ...FEATURES, '-o', devNull]);
  if (graph.status !== 0) throw new Error(`wasm-opt --print-call-graph failed: ${graph.stderr}`);
  // The printer writes names unescaped, so the graph is read only when the function map printed first, one
  // `index:name` line each, and the graph's nodes name the same distinct functions, none holding a quote: a line break
  // in a name adds a map line and splits the name's node and edge lines, and one before a line that reads as the
  // graph's header ends the map early.
  const parts = graph.stdout.replaceAll('\r\n', '\n').split(/^digraph call \{$/m);
  const names = parts[0].split('\n').slice(0, -1).map((line, i) => line.startsWith(`${i}:`) ? line.slice(`${i}:`.length) : null);
  const unreadable = () => new Unsplittable(`${named}: a function name holds a quote or a line break, which the call graph prints unescaped, so its calls cannot be read`);
  if (parts.length !== 2 || names.some(name => name === null || name.includes('"'))
    || new Set(names).size !== names.length) throw unreadable();
  const dot = parts[1];
  const nodes = new Map(), calls = new Map(), callers = new Map();
  // The graph's key is a subgraph of example nodes, which a function may share a name with ("B").
  let key = false;
  for (const line of dot.split('\n')) {
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
  if (nodes.size !== names.length || names.some(name => !nodes.has(name))) throw unreadable();
  if (start !== null) nodes.get(names[start]).root = true;
  return { nodes, calls, callers, start: start === null ? null : names[start] };
}

/** The index of the start function of the module in `bytes`, from its start section (id 8), or null. */
function startFunction(bytes) {
  let at = 8;
  const leb = () => {
    let value = 0;
    for (let shift = 0, byte = 0x80; byte & 0x80; shift += 7) { byte = bytes[at++]; value += (byte & 0x7f) * 2 ** shift; }
    return value;
  };
  while (at < bytes.length) {
    const id = bytes[at++], size = leb();
    if (id === 8) return leb();
    at += size;
  }
  return null;
}

/** Imports that let a split core instantiate outside the page: each `placeholder.<stage>` namespace the compiled
 * `module` imports, whose every placeholder is `call`. */
export function placeholderImports(module, call) {
  const namespaces = WebAssembly.Module.imports(module).map(entry => entry.module).filter(name => name.startsWith('placeholder.'));
  return Object.fromEntries(namespaces.map(name => [name, new Proxy({}, { get: () => call })]));
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
    const split = run('wasm-split', ['--multi-split', named, '--manifest', manifest, '--out-prefix', resolve(work, 'stage-'), '-o', resolve(work, 'core.wasm'), ...FEATURES]);
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
