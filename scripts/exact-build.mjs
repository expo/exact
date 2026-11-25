// Experimental (`EXACT_EXPERIMENTAL=1`): `share`'s iPhone build on Exact Build, a build service that turns an app's
// source into an unsigned IPA, for a machine that cannot build one itself (no Xcode, or Linux without the Apple cross
// SDK) or asked to with EXACT_BUILD=1. EXACT_BUILD_URL names another instance of the service.
//
// The upload is the app's files as git blob ids: the service asks for the ones it lacks. The build uses the SDK
// snapshot of this checkout's commit on main, and a checkout with changes of its own sends them as a patch against
// that snapshot. The service's last build of the app is the next one's base (`.exact/exact-build.json`), which it
// repacks in seconds when nothing native changed. The token, from an Expo device login, is ~/.exact-build-token.
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, readlinkSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { basename, dirname, relative, resolve, sep } from 'node:path';

export const SERVICE = 'https://exact.appdrop.app';
/** Never source: the app's build outputs and dependencies, at its top level (client.py's SKIP). */
const SKIP = new Set(['node_modules', '.exact', 'target', 'dist', '.git']);
const SDK_PATCH = '.exact-sdk.patch', MAX_PATCH = 8 * 1024 * 1024;
/** Each service's own token and pending login, so a token goes only to the service that issued it: Exact Build's in
 *  ~/.exact-build-token, another's (EXACT_BUILD_URL) in a file named by a digest of its URL. */
const ownFile = (env, name, ext = '') => {
  const url = serviceUrl(env);
  return resolve(env.HOME ?? homedir(), url === SERVICE ? `${name}${ext}` : `${name}-${createHash('sha256').update(url).digest('hex').slice(0, 16)}${ext}`);
};
const tokenPath = (env) => ownFile(env, '.exact-build-token');
const loginPath = (env) => ownFile(env, '.exact-build-login', '.json');

/** The service's origin: EXACT_BUILD_URL or Exact Build's own, refused unless HTTPS (plain HTTP to this machine only). */
export function serviceUrl(env = process.env) {
  const url = new URL(env.EXACT_BUILD_URL || SERVICE);
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)))
    throw new Error(`EXACT_BUILD_URL must be HTTPS: ${url.origin}`);
  return url.origin + url.pathname.replace(/\/$/, '');
}

/** The iOS build this machine is set up for, by what its user installed or configured, or null: Xcode on a Mac
 *  (DEVELOPER_DIR, Xcode.app, or an `xcodebuild` that answers), the Apple cross SDK on Linux (installed, or named by
 *  EXACT_APPLE_SDK, EXACT_APPLE_LLVM_BIN or EXACT_APPLE_CODESIGN). A setup that is there but incomplete still counts:
 *  its build says what it lacks. */
export function systemBuild(env = process.env, os = process.platform, run = spawnSync) {
  if (os === 'darwin') return env.DEVELOPER_DIR || existsSync('/Applications/Xcode.app') || run('xcodebuild', ['-version'], { env }).status === 0 ? 'Xcode' : null;
  if (os !== 'linux') return null;
  const swiftpm = env.XDG_CONFIG_HOME ? resolve(env.XDG_CONFIG_HOME, 'swiftpm') : resolve(env.HOME ?? homedir(), '.swiftpm');
  return env.EXACT_APPLE_SDK || env.EXACT_APPLE_LLVM_BIN || env.EXACT_APPLE_CODESIGN || existsSync(resolve(swiftpm, 'swift-sdks/darwin.artifactbundle')) ? 'the Apple cross SDK' : null;
}

/** Why share builds on Exact Build, or null for this machine's own build: only with EXACT_EXPERIMENTAL=1, and then
 *  with EXACT_BUILD=1 or where this machine has no iOS build of its own (`systemBuild`). */
export function exactBuildReason(env = process.env, os = process.platform, run = spawnSync) {
  if (env.EXACT_EXPERIMENTAL !== '1') return null;
  if (env.EXACT_BUILD === '1') return 'EXACT_BUILD=1';
  if (systemBuild(env, os, run)) return null;
  return `this machine has no iOS build of its own (${os === 'darwin' ? 'no Xcode' : os === 'linux' ? 'no Xcode or Apple cross SDK' : 'iOS builds need macOS or Linux'})`;
}

/** A request to the service; a 429 or 503 is retried after its Retry-After, for up to ten minutes. Redirects are
 *  refused, so the token goes nowhere else. Throws the service's error, with the status as `status`. */
async function call(base, path, { token, body, type, say, encoding } = {}) {
  const giveUp = Date.now() + 600_000;
  for (;;) {
    const response = await fetch(base + path, {
      method: body === undefined ? 'GET' : 'POST', body, redirect: 'error', signal: AbortSignal.timeout(60_000),
      headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), ...(type ? { 'Content-Type': type } : {}), ...(encoding ? { 'Content-Encoding': encoding } : {}) },
    });
    const wait = Number(response.headers.get('retry-after'));
    if ([429, 503].includes(response.status) && wait > 0 && Date.now() + wait * 1000 < giveUp) {
      const said = await response.json().catch(() => ({}));
      // A spent worker-time budget frees in hours: say when instead of waiting.
      if (said.window) throw Object.assign(new Error(`Exact Build: this account's ${said.window}ly build time is spent; it frees at ${said.available_at}`), { status: 429 });
      say?.(`share: Exact Build is busy (${response.status}); retrying in ${wait} s`);
      await Bun.sleep(Math.min(wait, 60) * 1000);
      continue;
    }
    if (response.ok) return response;
    const text = await response.text();
    let error = text;
    try { error = JSON.parse(text).error ?? text; } catch {}
    throw Object.assign(new Error(`Exact Build answered ${response.status} to ${path.split('?')[0]}: ${String(error).slice(0, 4000)}`), { status: response.status });
  }
}

/** The saved token, or null. */
export function token(env = process.env) {
  const path = tokenPath(env);
  const text = existsSync(path) ? readFileSync(path, 'utf8').trim() : '';
  return text.length >= 32 ? text : null;
}

/** Start an Expo device login: `{ url }`, the page a person opens to approve, its request kept for `finishLogin`. */
export async function startLogin(env = process.env) {
  const started = await (await call(serviceUrl(env), '/auth/device', { body: '' })).json();
  writeFileSync(loginPath(env), JSON.stringify({ service: serviceUrl(env), request_id: started.request_id, url: started.url }), { mode: 0o600 });
  return { url: started.url };
}

/** Finish the login `startLogin` began with the number the approving page shows; saves the token (mode 600). Returns
 *  `{ username }`, or `{ pending: seconds }` while the person has not approved yet. */
export async function finishLogin(match, env = process.env) {
  if (!existsSync(loginPath(env))) throw new Error('no Exact Build login was started here; run share again to start one');
  const { request_id: id, service } = JSON.parse(readFileSync(loginPath(env), 'utf8'));
  if (service !== serviceUrl(env)) { rmSync(loginPath(env), { force: true }); throw new Error('the Exact Build login here was started with another service; run share again to start one'); }
  let answer;
  try { answer = await (await call(serviceUrl(env), `/auth/device/${encodeURIComponent(id)}`, { body: JSON.stringify({ match: String(match) }), type: 'application/json' })).json(); }
  catch (error) { rmSync(loginPath(env), { force: true }); throw new Error(`${error.message}. Run share again to start a new login.`); }
  if (!answer.token) {
    if (['authorization_pending', 'slow_down'].includes(answer.status ?? answer.error)) return { pending: answer.retry_after ?? 5 };
    rmSync(loginPath(env), { force: true });
    throw new Error(`the login ended (${answer.status ?? answer.error ?? 'no token'}); run share again to start a new one`);
  }
  writeFileSync(tokenPath(env), answer.token, { mode: 0o600 });
  chmodSync(tokenPath(env), 0o600);
  rmSync(loginPath(env), { force: true });
  return { username: answer.username };
}

const git = (dir, args, extra = {}) => spawnSync('git', ['-C', dir, ...args], { encoding: 'buffer', maxBuffer: 2 * MAX_PATCH, env: { ...process.env, ...extra } });
const out = (result) => result.status === 0 ? result.stdout.toString().trim() : '';

/** The checkout's commit on expo/exact main, and its own changes outside `app` as a binary patch against the service's
 *  snapshot of that commit (`lookup`, GET /sdk's answer), or null when it has none (client.py `sdk_checkout`). */
export async function sdkSource(checkout, app, lookup) {
  const root = out(git(checkout, ['rev-parse', '--show-toplevel'])) || checkout, head = out(git(root, ['rev-parse', 'HEAD']));
  const remotes = out(git(root, ['remote', '-v'])).split('\n').map((line) => line.split(/\s+/));
  const remote = remotes.find(([name, url]) => /expo\/exact(\.git)?\/?$/i.test(url ?? '') && out(git(root, ['rev-parse', '-q', '--verify', `refs/remotes/${name}/main`])))?.[0]
    ?? (out(git(root, ['rev-parse', '-q', '--verify', 'refs/remotes/origin/main'])) ? 'origin' : null);
  const commit = remote ? out(git(root, ['merge-base', head, `refs/remotes/${remote}/main`])) || head : head;
  const inside = relative(root, app);
  const exclude = inside && !inside.startsWith('..') ? [`:(exclude)${inside.split(sep).join('/')}`] : [];
  const changed = commit !== head || out(git(root, ['status', '--porcelain', '--', '.', ...exclude])) !== '';
  const sdk = changed ? await lookup(commit) : null;
  if (!changed) return { commit, patch: null };
  if (!sdk) throw new Error('this Exact checkout has changes of its own, and the build service takes no SDK patch: share from an unchanged checkout');
  if (git(root, ['cat-file', '-e', `${sdk.upstream}^{commit}`]).status !== 0) {
    const fetched = git(root, ['fetch', '-q', remote ?? 'origin', sdk.upstream]);
    if (fetched.status !== 0) refuseGit(`fetch of the SDK snapshot's commit ${sdk.upstream}`, fetched);
  }
  // The working tree, untracked files too (.gitignore kept), through an index of its own: the checkout's is untouched.
  // It lives in the checkout's own Git directory, which a linked worktree's `.git` file only names.
  const gitDir = out(git(root, ['rev-parse', '--absolute-git-dir'])) || refuseGit('rev-parse --absolute-git-dir', git(root, ['rev-parse', '--absolute-git-dir']));
  const index = resolve(gitDir, `exact-build-index-${process.pid}`), env = { GIT_INDEX_FILE: index };
  const step = (args) => { const result = git(root, args, env); if (result.status !== 0) refuseGit(args.slice(0, 2).join(' '), result); };
  try {
    step(['read-tree', head]);
    step(['add', '-A', '--', '.', ...exclude]);
    const diff = git(root, ['diff', '--cached', '--full-index', '--binary', '--no-renames', '--no-ext-diff', '--no-textconv', sdk.upstream, '--', '.', ...exclude], env);
    if (diff.status !== 0) throw new Error(`git diff against the SDK snapshot failed: ${diff.stderr.toString().trim()}`);
    if (diff.stdout.length > MAX_PATCH) throw new Error(`this checkout's changes come to ${diff.stdout.length >> 10} KiB; the service takes ${MAX_PATCH >> 20} MiB at most`);
    return { commit, patch: diff.stdout.length ? diff.stdout : null };
  } finally { rmSync(index, { force: true }); }
}

const refuseGit = (what, result) => { throw new Error(`git ${what} in this Exact checkout failed: ${result.stderr.toString().trim() || `exit ${result.status}`}`); };

/** The app's files as `{ "<app>/<path>": [git blob id, mode] }` and each blob's bytes (client.py `manifest`). */
export function appFiles(dir) {
  const files = {}, blobs = new Map(), name = basename(dir);
  const add = (path, data, mode) => {
    const id = createHash('sha1').update(`blob ${data.length}\0`).update(data).digest('hex');
    files[`${name}/${path}`] = [id, mode];
    blobs.set(id, data);
  };
  const walk = (at, rel) => {
    for (const entry of readdirSync(at).sort()) {
      if ((!rel && (SKIP.has(entry) || entry === SDK_PATCH)) || entry.endsWith('.ipa')) continue;
      const path = resolve(at, entry), info = lstatSync(path), inner = rel ? `${rel}/${entry}` : entry;
      if (info.isSymbolicLink()) add(inner, Buffer.from(readlinkSync(path)), '120000');
      else if (info.isDirectory()) walk(path, inner);
      else if (info.isFile()) add(inner, readFileSync(path), info.mode & 0o111 ? '100755' : '100644');
    }
  };
  walk(dir, '');
  return { files, blobs, name };
}

/** Build `app` (resolveApp's) on the service into `ipa`, with `options` (POST /build's query: build number, version
 *  and the rest), from `checkout`, the Exact checkout. Reports on `say`. Throws with `code`: `exact_build_login` when
 *  there is no token (a login was started: the error names its page), `build_failed` when the app did not build. */
export async function buildIpa(app, ipa, { checkout, options = {}, env = process.env, say = () => {} } = {}) {
  const base = serviceUrl(env), saved = token(env);
  if (!saved) {
    const { url } = await startLogin(env);
    throw Object.assign(new Error(`share: Exact Build needs an Expo sign-in. Open ${url}, sign in to Expo and approve, then run \`bun exact.mjs share --login <the number it shows>\`.`), { code: 'exact_build_login', url });
  }
  const request = (path, init = {}) => call(base, path, { token: saved, say, ...init });
  const lookup = async (commit) => {
    try { return await (await request(`/sdk?exact=${commit}`)).json(); } catch (error) { if (error.status === 404) return null; throw error; }
  };
  const { commit, patch } = await sdkSource(checkout, app.dir, lookup);
  const record = resolve(app.dir, '.exact/exact-build.json');
  const last = existsSync(record) ? JSON.parse(readFileSync(record, 'utf8')) : null;
  // A release build is never repacked, so it names no base.
  const query = new URLSearchParams({ exact: commit, ...(last?.id && last.url === base && options.profile !== 'release' ? { base: last.id } : {}), ...options });
  const { files, blobs, name } = appFiles(app.dir);
  if (patch) { const id = createHash('sha1').update(`blob ${patch.length}\0`).update(patch).digest('hex'); files[`${name}/${SDK_PATCH}`] = [id, '100644']; blobs.set(id, patch); }
  say(`share: building on Exact Build (${base}) with Exact ${commit.slice(0, 9)}${patch ? `, and this checkout's changes (${patch.length >> 10} KiB)` : ''}`);
  const accepted = await (await request(`/build?${query}`, { body: JSON.stringify({ format: 1, files }), type: 'application/json' })).json();
  if (accepted.warning) say(`share: Exact Build: ${accepted.warning}`);
  // The blobs it lacks, in batches of about 16 MB: `<id> <size>\n<bytes>` each, gzipped.
  let batch = [], size = 0;
  const flush = async () => { if (batch.length) await request('/blobs', { body: Bun.gzipSync(Buffer.concat(batch), { level: 1 }), type: 'application/x-exact-blobs', encoding: 'gzip' }); batch = []; size = 0; };
  for (const id of accepted.missing ?? []) {
    const data = blobs.get(id);
    batch.push(Buffer.from(`${id} ${data.length}\n`), data); size += data.length;
    if (size > 16 * 1024 * 1024) await flush();
  }
  await flush();
  let status;
  for (const deadline = Date.now() + 30 * 60_000; ; await Bun.sleep(1000)) {
    status = await (await request(`/${accepted.id}/`)).json();
    if (status.status === 'failed') {
      for (const d of status.diagnostics ?? []) say(`${d.file}:${d.line}:${d.col} [${d.id}] ${d.message}`);
      if (!status.diagnostics?.length) say(String(status.error ?? '').slice(-8000));
      throw Object.assign(new Error(`share: the iPhone build on Exact Build failed (build ${accepted.id}); its errors are above.`), { code: 'build_failed' });
    }
    if (status.status === 'succeeded') break;
    if (Date.now() > deadline) throw Object.assign(new Error(`share: Exact Build build ${accepted.id} did not finish in 30 minutes`), { code: 'build_failed' });
  }
  for (const note of [status.note, status.sdk?.note]) if (note) say(`share: Exact Build: ${note}`);
  const bytes = Buffer.from(await (await request(`/${accepted.id}.ipa`)).arrayBuffer());
  if (status.bytes != null && bytes.length !== status.bytes) throw Object.assign(new Error(`share: the IPA from Exact Build is ${bytes.length} bytes, not the ${status.bytes} its status said`), { code: 'build_failed' });
  mkdirSync(dirname(ipa), { recursive: true });
  writeFileSync(`${ipa}.part`, bytes);
  renameSync(`${ipa}.part`, ipa);
  writeFileSync(record, `${JSON.stringify({ url: base, id: accepted.id }, null, 2)}\n`);
  say(`share: built on Exact Build (${status.repacked ? 'repacked from the last build' : status.repackReason ? `full build: ${status.repackReason}` : 'full build'})`);
}
