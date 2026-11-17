import { requireReleasePins } from '../game/inputs.mjs';
// `exact push` — the dogfood publisher: an edit to an app's Contract, TypeScript or Rust
// reaches its installed release binaries in seconds, as a signed update.
//
//   bun scripts/push.mjs <app> [--origin <dir|url>] [--channel <name>] [--keys <dir>] [--seq-above <n>] [--watch]
//
// `exact deploy` (deploy.mjs) is the publisher of record: it snapshots the
// source closure and classifies everything per platform. Push publishes the
// working tree's edit now, to every stream on the channel that can take it
// whole, or to none (LLP 1030.007 D1). Both call the publisher
// (publisher.mjs): each stream's signed head carries its cohort's receipt,
// a stream with no head yet starts from the receipt of the production build
// that made the installed binary (`build.mjs` with EXACT_UPDATE_TRUST=
// production writes it beside the bake), and push bakes only what an update
// carries, through the dev loop's resident Rust baker where it can. What a
// cohort cannot take (a new source shape, a grant, an executor) is refused
// with the classifier's reasons; that change needs a new binary.
//
// It publishes the working tree (the release record says `dirty`) and never
// the web root. `--watch` keeps the baker and the compiled modules resident
// and publishes after each save.
import { spawnSync } from 'node:child_process';
import { existsSync, watch } from 'node:fs';
import { homedir } from 'node:os';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Refusal, refuse, loadSigner } from './deploy-signing.mjs';
import { binaryCommand, defaultRelease, publishStream, publishTools, streamCandidates, streams } from './publisher.mjs';
import { classifyArtifacts, readBuilds, resolveApp } from './app.mjs';
import { RustBaker } from './rust.mjs';
import { channelOf, channelOrigin, openOrigin, validateChannel } from './origin.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const USAGE = 'usage: bun scripts/push.mjs <app> [--origin <dir|url>] [--channel <name>] [--variant <name>] [--keys <dir>] [--seq-above <n>] [--watch]';

function parseArgs(argv) {
  const opts = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--watch') opts.watch = true;
    else if (a === '--help' || a === '-h') opts.help = true;
    else if (['--origin', '--channel', '--variant', '--keys', '--seq-above'].includes(a)) {
      const value = argv[++i];
      if (!value || value.startsWith('-')) refuse(`${a} needs a value\n${USAGE}`);
      if (a === '--seq-above') {
        const n = Number(value);
        if (!/^\d+$/.test(value) || !Number.isSafeInteger(n) || n >= Number.MAX_SAFE_INTEGER) refuse('--seq-above needs a nonnegative safe integer with room for the next sequence');
        opts.seqAbove = n;
      } else opts[a.slice(2)] = value;
    } else if (a.startsWith('-')) refuse(`unknown flag ${a}\n${USAGE}`);
    else opts._.push(a);
  }
  if (opts._.length > 1) refuse(`unexpected argument ${opts._[1]}\n${USAGE}`);
  // As EXACT_VARIANT, which the flag replaces; resolveApp scopes it to the app (LLP 1030.008 D3).
  if (opts.variant) { process.env.EXACT_VARIANT = opts.variant; delete process.env.EXACT_VARIANT_APP; }
  return opts;
}

const seconds = (since) => `${((performance.now() - since) / 1000).toFixed(2)} s`;

function gitHead() {
  const r = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' });
  return r.status === 0 ? r.stdout.trim() : '0'.repeat(40);
}

/** Whether a save to `file` (relative to the app) changes what a push reads: a source by its type, or any file under
 * `assets/` or `deck/`, which `carriedAssets` carries whatever its type; never what a build or a tool writes. */
function pushReads(file) {
  if (/(^|\/)(target|node_modules|\.git|\.exact)(\/|$)/.test(file) || /(^|\/)\.DS_Store$/.test(file)) return false;
  return /^(assets|deck)\//.test(file) || /\.(contract|forma|rs|toml|json|png|jpe?g|webp|gif|svg|ttf|otf|mjs|ts|css|html)$/.test(file);
}

export async function pusher(opts) {
  const app = resolveApp(opts._[0]);
  requireReleasePins(app);
  const channel = validateChannel(opts.channel ?? channelOf(app.manifest));
  const spec = opts.origin ?? channelOrigin(app.manifest, channel);
  if (!spec) refuse(`no origin for the channel ${channel}: pass --origin <dir>, or name deploy.channels.${channel} ("appdrop" needs no server)`);
  const origin = openOrigin(spec);
  if (!origin.writable) refuse(`${origin.describe()} is read-only: pass --origin <the directory it serves>`);
  const signer = loadSigner(app, opts.keys ?? process.env.EXACT_SIGNING_KEY_DIR ?? resolve(homedir(), '.config/exact/keys'));
  origin.useSigner?.(signer);
  const signingKey = { id: signer.keyId, public: app.manifest.deploy?.signing?.keys?.[signer.keyId] };
  // Release bytes: what `deploy` builds, signed as it signs a macOS module.
  const env = { ...process.env, EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
  const tools = publishTools(app, env, existsSync(resolve(app.dir, 'app.ts')));
  const baker = new RustBaker(app, env);

  async function push() {
    requireReleasePins(app);
    const started = performance.now();
    const found = await streams(origin, app, channel, { builds: readBuilds(app, env) });
    const updates = await streamCandidates(app, origin, found, env, { baker, tools });
    const bakedIn = seconds(started);
    const release = defaultRelease(gitHead()).replace(/^r-/, 'p-');
    const snapshot = { id: gitHead(), commit: gitHead(), dirty: true };
    const name = (row) => `${row.platform} ${row.compatibilityId.slice(0, 8)}`;
    const listed = (items) => items.length > 6 ? `${items.slice(0, 6).join(', ')} and ${items.length - 6} more` : items.join(', ');
    // A stream whose compatibility inputs this source no longer computes has a binary that predates it: this edit
    // cannot reach it, and it stays as it is (LLP 1030.007 D3).
    const frozen = updates.filter((update) => update.superseded?.length);
    const current = updates.filter((update) => !update.superseded?.length);
    const why = (update) => `its binary predates these sources (${listed(update.superseded)}); ${binaryCommand(app, update.row.platform)} starts a new stream`;
    if (!current.length) refuse(`no stream on the channel ${channel} can take this as an update: ${frozen.map((update) => `${name(update.row)}: ${why(update)}`).join('; ')}`);
    for (const update of frozen) console.log(`  ${name(update.row)} — frozen: ${why(update)}`);
    // Every current stream is checked before any is published.
    for (const update of current) {
      const checked = classifyArtifacts(update.build, update.row.receipt, signingKey);
      if (update.reason || !checked.bundle) refuse(`${name(update.row)} cannot take this as an update — ${update.reason ?? checked.missing.join('; ')}. It needs ${binaryCommand(app, update.row.platform)}`);
    }
    for (const update of current) {
      const native = update.build.pendingInputs ?? [];
      if (native.length) console.log(`  ${name(update.row)} — note: native sources differ from its binary's, and only a new binary carries them: ${listed(native)}`);
      if (update.note) console.log(`  note: ${update.note}`);
    }
    const results = [];
    for (const update of current) results.push(await publishStream({ origin, ...update, app, signer, release, snapshot, opts: { seqAbove: opts.seqAbove }, log: () => {} }));
    for (const r of results) {
      const what = r.action === 'current' ? 'already current' : `seq ${r.seq}: ${r.changes.map((c) => c.name).join(', ')}`;
      console.log(`  ${r.platform} ${r.compatibilityId.slice(0, 8)} — ${what}`);
    }
    console.log(`pushed ${release} in ${seconds(started)} (baked in ${bakedIn})`);
  }

  try {
    await push();
    if (!opts.watch) return 0;
    console.log(`watching ${app.dir} — save to push`);
    let timer = null, running = null, again = false;
    const kick = () => {
      if (running) { again = true; return; }
      running = push().catch((e) => console.error(`push refused: ${e.message}`)).finally(() => {
        running = null;
        if (again) { again = false; kick(); }
      });
    };
    watch(app.dir, { recursive: true }, (_, file) => {
      if (!file || !pushReads(file)) return;
      clearTimeout(timer);
      timer = setTimeout(() => { console.log(`edit: ${relative(app.dir, resolve(app.dir, file))}`); kick(); }, 60);
    });
    await new Promise(() => {});
  } finally { baker.close(); }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const run = async () => {
    const opts = parseArgs(process.argv.slice(2));
    if (opts.help || !opts._.length) { console.log(USAGE); return opts.help ? 0 : 2; }
    return pusher(opts);
  };
  run().then((code) => { process.exitCode = code; }, (e) => {
    console.error(e instanceof Refusal ? `exact push: ${e.message}` : e);
    process.exitCode = 1;
  });
}
