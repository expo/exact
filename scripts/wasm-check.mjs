// The hourly lane checks the workspace libraries and leaves that web apps link for wasm32.
// A native build cannot find cfg-only refusals on this target.
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertOwnTarget } from './app.mjs';

const TARGET = 'wasm32-unknown-unknown';
// Leaves the JS target builds for wasm beside the apps' graph (host/web-js/module.mjs, build.mjs): the markdown
// editor, the motion module and the text flow's binary. (A second on top of the apps' graph; the GPU leaves are not.)
const LIBS = ['exact-markdown-editor', 'exact-web-js-motion'];
const BINS = [['exact-textflow', 'textflow-web']];

/** A command's result, or a throw naming it: a spawn error, or output past the buffer (ENOBUFS, which can come
 * with status 0 and output cut short), is never a result. */
function run(root, cmd, args, { ok = [0] } = {}) {
  const r = spawnSync(cmd, args, { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  if (r.error) throw new Error(`${cmd} ${args[0]}: ${r.error.message}`);
  if (!ok.includes(r.status)) throw new Error(`${cmd} ${args[0]} exited ${r.status}: ${(r.stderr ?? '').trim().split('\n').slice(-3).join(' ')}`);
  return r;
}

/** The workspace's library crates in the wasm32 graph of its web apps (`apps/<app>/web`, or `apps/<app>/app/web`
 * for an app beside its Snapback backend, Harmony's layout), the apps and vendored
 * crates aside; and Cargo's target directory. Throws rather than answering none. */
export function wasmCrates(root) {
  const meta = JSON.parse(run(root, 'cargo', ['metadata', '--no-deps', '--offline', '--format-version', '1']).stdout);
  const top = meta.workspace_root.replaceAll('\\', '/'), members = new Set(meta.workspace_members);
  const rel = (p) => p.manifest_path.replaceAll('\\', '/').slice(top.length + 1, -'/Cargo.toml'.length);
  const local = meta.packages.filter((p) => members.has(p.id)).map((p) => ({ name: p.name, dir: rel(p), p }));
  const apps = local.filter(({ dir }) => /^apps\/[^/]+(\/app)?\/web$/.test(dir));
  if (!apps.length) throw new Error('no apps/<app>/web (or apps/<app>/app/web) crate in the workspace');
  const tree = run(root, 'cargo', ['tree', ...apps.flatMap((a) => ['-p', a.name]), '--target', TARGET, '-e', 'normal', '--prefix', 'none', '--offline', '--locked']);
  const named = new Set(tree.stdout.split('\n').map((l) => /^(\S+) v/.exec(l)?.[1]).filter(Boolean));
  const gone = LIBS.filter((name) => !local.some((c) => c.name === name));
  if (gone.length) throw new Error(`a wasm leaf is gone: ${gone.join(', ')}`);
  const crates = local.filter(({ name, dir }) => (named.has(name) || LIBS.includes(name)) && !/^(apps|vendor)\//.test(dir))
    .filter(({ p }) => p.targets.some((t) => t.kind.some((k) => k.endsWith('lib')))).map(({ name }) => name).sort();
  if (!crates.includes('exact-web')) throw new Error(`the web apps' wasm32 graph has no exact-web (found ${crates.length} crates)`);
  const bins = BINS.filter(([pkg, bin]) => meta.packages.some((p) => members.has(p.id) && p.name === pkg && p.targets.some((t) => t.name === bin)));
  if (bins.length !== BINS.length) throw new Error(`a wasm leaf is gone: ${BINS.filter((b) => !bins.includes(b)).map(([p, b]) => `${p} --bin ${b}`).join(', ')}`);
  const dirs = local.filter(({ name }) => crates.includes(name) || bins.some(([pkg]) => pkg === name)).map(({ dir }) => dir + '/');
  return { crates, bins, dirs, roots: apps.map(({ dir }) => dir), target: meta.target_directory };
}

/** `{ problems: [{ code, message, detail }], counted }`, with every diagnostic and rerun. */
export function wasmCheck(root, { env = process.env, found: given, execute = run } = {}) {
  const problem = (message, detail) => ({ problems: [{ code: 'wasm:check', message, detail }], counted: 'wasm32: not checked' });
  let found;
  try {
    found = given ?? wasmCrates(root);
    found.build = env.CARGO_BUILD_BUILD_DIR ? resolve(root, env.CARGO_BUILD_BUILD_DIR) : found.target; // (a config file's build-dir is unstable Cargo's)
    assertOwnTarget(found.target, root);
    if (found.build !== found.target) assertOwnTarget(found.build, root);
  } catch (e) { return problem(`the crates the web apps link could not be listed, so none was checked for wasm32: ${e.message}`); }
  const checks = [['--lib', '--keep-going', ...found.crates.flatMap((c) => ['-p', c])], ...found.bins.map(([pkg, bin]) => ['-p', pkg, '--bin', bin])];
  const results = checks.map((args) => {
    try { return { args, ...execute(root, 'cargo', ['check', '--offline', '--locked', '--target', TARGET, '--message-format', 'short', ...args], { ok: [0, 101] }) }; } catch (e) { return { args, thrown: e.message }; }
  });
  return report(results, found, 'Cargo decides what changed');
}

/** The report of the checks' `results` (`{ args, status, stderr }`, or `{ args, thrown }` for one that could not
 * run): every diagnostic, a build script's failure with its cause, and a rerun for each check that failed. */
export function report(results, found, why) {
  const count = `${found.crates.length} crates and ${found.bins.length} binary the web apps link`;
  if (results.every((r) => r.status === 0 && !r.thrown)) return { problems: [], counted: `wasm32: ${count} checked for ${TARGET} (${why}), 0 errors` };
  const errors = [], reruns = [], failed = new Set();
  for (const r of results) {
    if (r.thrown) { errors.push(`could not run: ${r.thrown}`); reruns.push(r.args); continue; }
    if (r.status === 0) continue;
    const lines = (r.stderr ?? '').split('\n'), mine = [];
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      if (/^error: (could not compile|aborting|build failed)/.test(l)) { const c = /`([^`]+)`/.exec(l)?.[1]; if (c) failed.add(c); continue; }
      if (!/(^|: )error(\[E\d+\])?:/.test(l)) continue;
      // A diagnostic at its file is one line (`--message-format short`); Cargo's own (a build script's failure) is
      // its block, the cause beneath it, up to the next diagnostic
      let block = l;
      if (/^error:/.test(l)) { for (let j = i + 1; j < lines.length && j < i + 40 && !/^(error|warning)(\[|:)/.test(lines[j]); j++) if (lines[j].trim()) block += `\n      ${lines[j].trim()}`; }
      const built = /^error: failed to run custom build command for `(\S+)/.exec(l)?.[1]; if (built) failed.add(built);
      mine.push(block);
    }
    if (!mine.length) mine.push(`exited ${r.status} with no diagnostic: ${lines.filter(Boolean).slice(-4).join(' / ')}`);
    errors.push(...mine.filter((e) => !errors.includes(e)));
    reruns.push(r.args[0] === '--lib' && failed.size ? ['--lib', ...[...failed].flatMap((c) => ['-p', c])] : r.args.filter((a) => a !== '--keep-going'));
  }
  const cfg = errors.some((l) => /error\[E0(425|433|412|432)\]/.test(l)) ? ' A name only a cfg(unix) or cfg(windows) arm gives needs an arm for neither.' : '';
  return {
    problems: [{ code: 'wasm:check', message: `cargo check --target ${TARGET} failed for the crates the web apps link: ${errors.length} error${errors.length === 1 ? '' : 's'}${failed.size ? ` in ${[...failed].join(', ')}` : ''}.${cfg}`,
      detail: `${errors.join('\n    ')}\n    Rerun: ${reruns.map((a) => `cargo check --target ${TARGET} ${a.join(' ')}`).join('; ')}` }],
    counted: `wasm32: ${count} checked (${why}), ${errors.length} error${errors.length === 1 ? '' : 's'}`,
  };
}

if (import.meta.main) {
  if (process.argv.includes('--help')) console.log('Usage: bun scripts/wasm-check.mjs\nCheck the web libraries and wasm leaves; the hourly lane owns this compile.');
  else {
    const result = wasmCheck(resolve(fileURLToPath(new URL('..', import.meta.url))));
    console.log(result.counted);
    for (const p of result.problems) console.error(`${p.message}\n${p.detail ?? ''}`);
    process.exitCode = result.problems.length ? 1 : 0;
  }
}
