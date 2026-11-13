// The web host's own modules the JS target's runtime loads, copied beside it,
// and the rewrites that point the runtime's `../web/…` imports at those copies.
// One list for every runtime staged from host/web-js: a page (build.mjs) and
// the differential driver's headless runtime (semantics/difftest/js/drive.mjs),
// which lost the bundle when it kept a list of its own (intersect-glue.js,
// 2026-10-12). The async lane's `difftest-js` inputs read it too.
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';

export const HOST_WEB_GLUE = [
  // Loaded after first paint: motion.js, a pan, `select`, text flow, rt.js `pr`, native.js, rt.js `geo`, media.js, sounds.js; wakelock.js imports wakelock-glue.js, orientationlock.js orientationlock-glue.js.
  'frames.js', 'motion-glue.js', 'group-glue.js', 'input-glue.js', 'agent-input.js', 'markup-editor.js', 'atoms.js', 'decorations.js', 'textflow-glue.js', 'timer-glue.js', 'timer-waits-glue.js', 'timer-observations.js',
  'presence-glue.js', 'native-glue.js', 'geometry-glue.js', 'resize-glue.js', 'intersect-glue.js', 'media-glue.js',
  'sound-glue.js', 'midi-glue.js', 'prompt-glue.js', 'alert-message.js', 'wakelock-glue.js', 'orientationlock-glue.js', 'menu-rows.js',
  // Virtualized lists' browser half; animated images on the agent's clock (agent.js only); Canvas 2D's replayer.
  'collection-glue.js', 'image-glue.js', 'canvas2d-glue.js',
  // An iframe's navigation records and commands (commands.js, rt.js `onFrameNavigation`; LLP 1020.002).
  'frame-navigation.js', 'frame-messages.js',
  // Sign-in, storage, request bodies, injected faults and the local network's spaces (ts-data.js, auth.js, admission.js, agent.js, modules.js).
  'auth-glue.js', 'storage-environment.js', 'http-body.js', 'faults.js', 'address-space.js',
  // The scroll to a location's fragment, at its first (router.js; LLP 1153 D5).
  'fragment.js',
];

/** Copy the host/web glue from the repository at `root` into `dir`. */
export function stageHostGlue(root, dir) {
  for (const f of HOST_WEB_GLUE) cpSync(resolve(root, 'host/web', f), resolve(dir, f));
}

/**
 * The differential driver's runtime inputs, relative to the repository at `root`:
 * every host/web-js module, the glue, and navigation's source graph. The driver
 * bundles navigation and admission; the async lane's `difftest-js` watches these inputs.
 */
export function driverRuntime(root) {
  const own = readdirSync(resolve(root, 'host/web-js')).filter(f => f.endsWith('.js')).sort().map(f => `host/web-js/${f}`);
  return [...new Set([...own, ...HOST_WEB_GLUE.map(f => `host/web/${f}`),
    ...hostModuleSources(root, ['host/web/navigation.js'])])];
}

/** A runtime module's `../web/…` imports, pointed at the copies beside it. */
export const localImports = text => text
  .replaceAll("'../web/grant-admission.js'", "'./grant-admission.js'")
  .replaceAll("'../web/faults.js'", "'./faults.js'")
  .replaceAll("'../web/address-space.js'", "'./address-space.js'")
  .replaceAll('"../web/address-space.js"', '"./address-space.js"');

// @ref LLP 1043.000 §3 D7/D8 — one inventory for host builds, serving and fixtures.
// Groups preserve capability-based shipping; none of these imports enters boot.
// A group's files import statically only from it or base (web-host-groups.test.mjs): base's documents-glue.js
// reads storage-environment.js, so it ships in base, not only with storage.
const WEB_HOST_GROUPS = {
  base: ['glue.js', 'navigation.js', 'audio-session.js', 'frame-messages.js', 'frame-navigation.js', 'textflow-glue.js', 'timer-glue.js', 'timer-waits-glue.js', 'timer-observations.js', 'input-glue.js', 'agent-input.js',
    'http-body.js', 'grant-admission.js', 'faults.js', 'address-space.js', 'media-glue.js', 'list-selection.js', 'markup-editor.js', 'atoms.js', 'decorations.js', 'live-time-glue.js', 'document-glue.js',
    'motion-glue.js', 'group-glue.js', 'collection-glue.js', 'canvas2d-glue.js', 'presence-glue.js', 'picker-glue.js',
    'documents-glue.js', 'storage-environment.js', 'auth-glue.js', 'image-glue.js', 'geometry-glue.js', 'resize-glue.js', 'intersect-glue.js', 'sound-glue.js', 'midi-glue.js', 'prompt-glue.js', 'alert-message.js', 'wakelock-glue.js', 'orientationlock-glue.js', 'menu-rows.js', 'fragment.js'],
  module: ['module-glue.js', 'module-worker.js', 'module-binary.js', 'module-prelude.js'],
  storage: ['storage-request.js', 'storage.js', 'storage-fs.js', 'storage-image.js', 'storage-sqlite.js',
    'storage-worker.js', 'sqlite3.mjs', 'sqlite3.wasm'],
  rust: ['rust-glue.js'],
  gpu: ['gpu-glue.js', 'pace.js', 'gpu-assets.js', 'gpu-pads.js', 'gpu-tier.js', 'gpu-refusal.js'],
  gpuModules: ['gpu-modules.js'],
  native: ['native-glue.js'],
  // A development page's frame sampler (LLP 1079 D3); a production build ships none.
  development: ['frames.js'],
};

/** Public filename -> repo-relative source; omit groups to inventory every host file. */
export function webHostFiles(...groups) {
  return Object.fromEntries((groups.length ? groups : Object.keys(WEB_HOST_GROUPS))
    .flatMap(group => WEB_HOST_GROUPS[group]).map(name => [name,
      name === 'module-binary.js' ? 'js/src/binary.js'
        : name.startsWith('sqlite3.') ? 'node_modules/@sqlite.org/sqlite-wasm/dist/' + (name === 'sqlite3.mjs' ? 'index.mjs' : name)
          : ['glue.js', 'navigation.js', 'grant-admission.js', 'module-prelude.js'].includes(name) ? 'target/web-host/' + name : 'host/web/' + name]));
}

/** The data module realm's prelude (LLP 1027 D1a): these scripts, in this order, after module-binary.js, as
 * js/build.rs joins them natively. The realm evaluates them as one script, `module-prelude.js`, whose hash the
 * served page's CSP names; each file is a script of its own whose prelude.js takes the parts the others leave. */
export const MODULE_PRELUDE = ['time.js', 'crypto.js', 'module-work.js', 'fetch.js', 'auth.js', 'store.js', 'storage.js', 'reply.js', 'prelude.js'].map(f => 'js/src/' + f);
/** `module-prelude.js`'s text, from the repository at `root`. */
export const modulePrelude = root => MODULE_PRELUDE.map(f => readFileSync(resolve(root, f), 'utf8')).join('\n');


/** The two entry modules' static sources, for build invalidation. */
export function hostModuleSources(root, entries = ['host/web/glue.js', 'host/web/navigation.js']) {
  const pending = [...entries];
  const seen = new Set();
  const parser = new Bun.Transpiler({ loader: 'js' });
  while (pending.length) {
    const file = pending.pop();
    if (seen.has(file)) continue;
    seen.add(file);
    for (const request of parser.scanImports(readFileSync(resolve(root, file), 'utf8'))) {
      if (request.kind === 'import-statement') pending.push(relative(root, resolve(root, dirname(file), request.path)));
    }
  }
  return [...seen];
}

// Gate sources before bundling: Rolldown can simplify a true branch even with
// constant inlining disabled. Lazy modules pass through the same gate (LLP 1069.007 D2).
export function gateAgent(code, name) {
  const declaration = /\b(?:const|let|var) AGENT_ADMITTED(?:\$\d+)? = (?:true|false);/g;
  const imported = /\bimport\s*\{[^}]*\bAGENT_ADMITTED\b[^}]*\}\s*from/;
  if (/searchParams\.has\(["']agent["']\)|params\.has\(["']agent["']\)|URLSearchParams\([^)]*\)\.has\(["']agent["']\)/.test(code)
      && !new RegExp(declaration.source).test(code) && !imported.test(code)) {
    throw new Error(`${name} reads ?agent without AGENT_ADMITTED; a production build must not admit agent mode (LLP 1069.007 D2)`);
  }
  return code.replace(declaration, text => text.replace('= true', '= false'));
}

/** Bundle one host entry; the wasm glue keeps navigation as its other shipped module. */
export async function bundleHostModule(root, name, { grants = false, production = false } = {}) {
  const web = resolve(root, 'host/web');
  const external = name === 'glue.js' ? 'navigation.js' : grants ? 'grant-admission.js' : null;
  const { rolldown } = await import('rolldown');
  const bundle = await rolldown({
    input: resolve(web, name),
    treeshake: false,
    optimization: { inlineConst: false },
    plugins: [{
      name: 'host-module-boundary',
      transform(code, id) {
        if (production) return gateAgent(code, id);
      },
      resolveId(source, importer) {
        if (external && importer && resolve(dirname(importer), source) === resolve(web, external)) {
          return { id: './' + external, external: true };
        }
      },
    }],
  });
  try {
    const { output } = await bundle.generate({ format: 'es', comments: false });
    if (output.length !== 1) throw new Error(`${name}: expected one host module, got ${output.length}`);
    return output[0].code;
  } finally {
    await bundle.close();
  }
}

/** Build the host files consumed by the wasm build, servers and bare-plan fixtures. */
export async function buildHostGlue(root, { production = false, out = resolve(root, 'target/web-host') } = {}) {
  mkdirSync(out, { recursive: true });
  const stage = mkdtempSync(resolve(out, '.build-'));
  try {
    for (const name of ['glue.js', 'navigation.js']) {
      writeFileSync(resolve(stage, name), await bundleHostModule(root, name, { production }));
    }
    const admission = await import(resolve(root, 'host/web/grant-admission.js'));
    writeFileSync(resolve(stage, 'grant-admission.js'), `export { ${Object.keys(admission).join(', ')} } from './navigation.js';\n`);
    writeFileSync(resolve(stage, 'module-prelude.js'), modulePrelude(root));
    // Readers may already have a published module open while another build finishes.
    for (const name of ['glue.js', 'navigation.js', 'grant-admission.js', 'module-prelude.js']) {
      renameSync(resolve(stage, name), resolve(out, name));
    }
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}
