#!/usr/bin/env bun
// `bun exact.mjs share`: build the app for iPhone and print an AppDrop link that
// installs it. The person opens the link on their iPhone, and AppDrop signs the
// app with their own Apple ID: no TestFlight, provisioning profile or cable.
//
//   bun scripts/share.mjs --app <name> [--new-link | --force] [--json]
//
// The link is the last line of stdout; the build and the upload report on
// stderr. Each checkout keeps its link in the app's `.exact/appdrop.json`
// (gitignored), with the git branch it was published from. The first share
// creates the link; later ones publish a new version to it, so a phone that
// installed it sees "Update available". A share from another branch refuses
// (code `link_branch_mismatch`): `--new-link` gives this branch its own link,
// `--force` publishes over the other branch's. When the app's git tree and
// exact2's are clean and at the commits of the last share, or past them only by
// the build number that share wrote back, nothing is built and the same link is
// printed.
//
// The build carries the app's version (`version` in app.json) and the next build
// number past `buildNumber` and the link's last build, which AppDrop shows in its
// version fields; the share writes that number back to app.json, to be committed.
// The upload is the appdrop CLI's (`npx appdrop`, or EXACT_APPDROP_CLI), run
// outside any git checkout, so the link carries no branch label beside them. Its
// update key stays in the CLI's own ~/.appdrop/drops.json and never reaches stdout.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { guardFlags } from './help.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const CLI = process.env.EXACT_APPDROP_CLI ?? 'appdrop@0.8';

const git = (dir, args) => {
  // Paths unquoted, whatever their characters
  const r = spawnSync('git', ['-c', 'core.quotePath=false', '-C', dir, ...args], { encoding: 'utf8' });
  return r.status === 0 ? r.stdout.trim() : null;
};

/** The branch `dir` is on; a detached HEAD's short commit; null outside git. */
export function branchOf(dir) {
  return git(dir, ['symbolic-ref', '--short', '-q', 'HEAD']) || git(dir, ['rev-parse', '--short', 'HEAD']);
}

/** What the build is made from, when both trees are clean but for the `build` number the last share wrote back to
 *  app.json, which is not yet committed: the app's commit and exact2's. Else null. */
export function sourceOf(dir, exact2 = ROOT, build = null) {
  const manifest = git(dir, ['ls-files', '--full-name', 'app.json']);
  const oneRepository = git(dir, ['rev-parse', '--show-toplevel']) === git(exact2, ['rev-parse', '--show-toplevel']);
  // `-z` leaves paths unquoted and ends each entry with a NUL; `git` trims the leading space, so one modified file,
  // staged or not, reads `M <path>`
  const writtenBack = (status) => build != null && manifest !== null && /^M\s+/.test(status) && status.replace(/^M\s+/, '') === manifest
    && readFileSync(resolve(dir, 'app.json'), 'utf8').trim() === withBuildNumber(git(dir, ['show', `HEAD:${manifest}`]) ?? '', build);
  const parts = [];
  for (const tree of [dir, exact2]) {
    const commit = git(tree, ['rev-parse', 'HEAD']), status = git(tree, ['status', '--porcelain', '-z', '--', '.'])?.replace(/\0$/, '');
    if (!commit || status == null || (status !== '' && !((tree === dir || oneRepository) && writtenBack(status)))) return null;
    parts.push(commit);
  }
  return parts.join(' ');
}

/** A share link (`https://<host>/d/<id>`) as `{ url, origin, id }`, or null. */
export function parseLink(url) {
  const parsed = URL.canParse(url) ? new URL(url) : null;
  const id = parsed?.pathname.match(/^\/d\/([A-Za-z0-9]+)\/?$/)?.[1];
  return parsed?.protocol === 'https:' && id ? { url: `${parsed.origin}/d/${id}`, origin: parsed.origin, id } : null;
}

/** What this share does, from the checkout's record, the branch and the source:
 *  `create`, `update`, `current` (nothing to build) or `refuse`. `same` says whether the recorded source and this
 *  one hold the same build (`sameBuild`). */
export function plan(record, { branch, source, newLink = false, force = false, same = (a, b) => a === b }) {
  const link = !newLink && record?.url ? parseLink(record.url) : null;
  if (!link) return { action: 'create' };
  if (record.branch && branch && record.branch !== branch && !force) return { action: 'refuse', ...link, from: record.branch };
  if (source && record.source && same(record.source, source) && record.branch === branch) return { action: 'current', ...link, revision: record.revision };
  return { action: 'update', ...link };
}

/** Whether source `now` holds the build source `recorded` made: the same commits, or commits that change only
 *  `dir`'s app.json, and only its `buildNumber` to `build`, as committing a share's write-back does. An exact2
 *  outside the app's repository must be at the same commit. */
export function sameBuild(dir, recorded, now, build, exact2 = ROOT) {
  if (recorded === now) return true;
  const manifest = git(dir, ['ls-files', '--full-name', 'app.json']);
  if (!build || !manifest) return false;
  const was = recorded.split(' '), is = now.split(' ');
  const oneRepository = git(dir, ['rev-parse', '--show-toplevel']) === git(exact2, ['rev-parse', '--show-toplevel']);
  return [dir, exact2].every((tree, i) => {
    if (was[i] === is[i]) return true;
    if (i === 1 && !oneRepository) return false;
    if (git(tree, ['diff', '--no-relative', '--name-only', was[i], is[i]]) !== manifest) return false;
    const before = git(tree, ['show', `${was[i]}:${manifest}`]), after = git(tree, ['show', `${is[i]}:${manifest}`]);
    return before !== null && after !== null && after === withBuildNumber(before, build);
  });
}

/** The version and build number a share gives the app, which AppDrop shows: `manifest.version` (null for the
 *  build's default), and the next number past both `manifest.buildNumber` and `published`, the link's last build,
 *  since AppDrop refuses a build number it already published. */
export function numbers(manifest, published = 0) {
  return { version: manifest.version ?? null, build: Math.max(manifest.buildNumber ?? 0, published) + 1 };
}

/** The link's last build number, from AppDrop's answer for it: its `version` (the build number a share gave it),
 *  or its revision for a link whose builds AppDrop numbered. */
export function publishedBuild(drop) {
  return Math.max(drop.revision ?? 1, Number.parseInt(drop.version, 10) || 0);
}

/** `text`, an app.json, with its `buildNumber` set to `build`, the rest as written; `text` unchanged when it names
 *  no `buildNumber`, as an app that does not commit its numbers. */
export function withBuildNumber(text, build) {
  return text.replace(/("buildNumber"\s*:\s*)\d+/, `$1${build}`);
}

/** Why `share` cannot build `manifest`'s app, or null. A tester's build starts the app's own update stream when it
 *  has an update store, and a production build with an update store needs a key to verify what it takes
 *  (bake/src/compat.rs): an app with neither keys nor `deploy.store.ios: "0"` would fail minutes into the build. */
export function unsignedStore(manifest) {
  const keys = manifest.deploy?.signing?.keys;
  if (manifest.deploy?.store?.ios === '0' || (keys && typeof keys === 'object' && Object.keys(keys).length)) return null;
  return 'share: this app takes over-the-air updates on iPhone (it says nothing of deploy.store.ios), and a shared '
    + 'build of one needs a key to verify them, which app.json does not name. For a build that takes no updates, add '
    + '"deploy": { "store": { "ios": "0" } } to app.json; to keep updates, name a verification key in '
    + 'deploy.signing.keys.';
}

/** The refusal for a link another branch published, as the agent reads it. */
export const mismatch = (p, branch) => `This app's AppDrop link, ${p.url}, was last shared from ${p.from}; this checkout is on ${branch}. `
  + `Pass --new-link to give ${branch} its own link, or --force to publish over ${p.from}'s.`;

async function main(argv) {
  guardFlags('share', argv);
  const name = argv[argv.indexOf('--app') + 1];
  const json = argv.includes('--json'), newLink = argv.includes('--new-link'), force = argv.includes('--force');
  const say = (line) => console.error(line);
  const fail = (code, error, extra = {}) => {
    if (json) console.log(JSON.stringify({ code, error, ...extra }));
    say(error);
    return 1;
  };
  if (newLink && force) return fail('share_flags', 'share: --new-link and --force are opposites; pass one.');
  const { resolveApp } = await import('./app.mjs');
  const app = resolveApp(name);
  if (app.manifest.variant) return fail('share_variant', `share: variant ${app.manifest.variant.name}: a variant has no AppDrop link of its own (LLP 1030.008 §6); share the app without --variant or EXACT_VARIANT.`);
  const recordPath = resolve(app.dir, '.exact/appdrop.json');
  const record = existsSync(recordPath) ? JSON.parse(readFileSync(recordPath, 'utf8')) : null;
  // A build's version is the manifest's, else one the environment gives the build, else the build's default
  const shown = app.manifest.version ?? process.env.EXACT_VERSION ?? '0.1.0';
  const branch = branchOf(app.dir), source = sourceOf(app.dir, ROOT, record?.build);
  const same = (recorded, now) => (record.version ?? shown) === shown && sameBuild(app.dir, recorded, now, record.build);
  const step = plan(record, { branch, source, newLink, force, same });
  if (step.action === 'refuse') return fail('link_branch_mismatch', mismatch(step, branch), { url: step.url, branch: step.from });
  if (step.action === 'current') {
    say(`share: nothing changed since build ${record.build ?? step.revision} of ${step.url}; not rebuilt.`);
    console.log(json ? JSON.stringify({ url: step.url, revision: step.revision, version: record.version, build: record.build, branch, action: 'current' }) : step.url);
    return 0;
  }

  const unsigned = unsignedStore(app.manifest);
  if (unsigned) return fail('update_keys_missing', unsigned);

  // AppDrop refuses a build number it already published
  let revision = 0, published = 0;
  if (step.action === 'update') {
    const drop = await fetch(`${step.origin}/api/drops/${step.id}`, { cache: 'no-store', signal: AbortSignal.timeout(30000) }).catch((e) => ({ ok: false, status: e.message }));
    if (drop.status === 404) return fail('link_gone', `share: ${step.url} no longer exists (deleted?). Pass --new-link to make another.`);
    if (!drop.ok) return fail('appdrop_unavailable', `share: AppDrop answered ${drop.status} for ${step.url}; try again.`);
    const last = await drop.json();
    revision = last.revision ?? 1;
    published = publishedBuild(last);
  }
  const { version, build } = numbers(app.manifest, published);

  const ipa = resolve(app.dir, '.exact/share', `${app.name}.ipa`);
  rmSync(ipa, { force: true });
  mkdirSync(dirname(ipa), { recursive: true });
  // The bake bundles app.ts with rolldown from this checkout's node_modules; without it Bun spins silently.
  if (!['rolldown', 'rolldown.exe'].some((bin) => existsSync(resolve(ROOT, 'node_modules/.bin', bin))))
    return fail('node_modules_missing', `share: exact's node_modules is missing: run \`bun install --frozen-lockfile\` in ${ROOT} (every build's type check imports rolldown from it)`);
  say(`share: building ${app.displayName ?? app.name} ${shown} (${build}) for iPhone; the first build takes minutes.`);
  // An app with an update store starts its own stream: this build is a tester's, not a release in one.
  const streamed = app.manifest.deploy?.store?.ios !== '0';
  const built = spawnSync(process.execPath, [resolve(ROOT, 'host/apple/build.mjs'), '--device', app.crate('apple'), '--archive', ipa, '--unsigned'], {
    cwd: ROOT, stdio: ['ignore', 2, 2],
    env: { ...process.env, EXACT_APP_DIR: app.dir, EXACT_BUILD_NUMBER: String(build), ...(version ? { EXACT_VERSION: version } : {}), ...(streamed ? { EXACT_UPDATE_GENESIS: '1' } : {}) },
  });
  if (built.status !== 0 || !existsSync(ipa)) return fail('build_failed', `share: the iPhone build failed (${built.error?.message ?? `exit ${built.status}`}); its errors are above.`);

  say(step.action === 'create' ? 'share: uploading to a new AppDrop link' : `share: uploading to ${step.url}`);
  // Run outside the app's checkout, whose branch the CLI would take as the link's label
  const uploaded = spawnSync('npx', ['--yes', CLI, ipa, ...(step.action === 'update' ? ['--update', step.url] : []), '--json'], {
    cwd: tmpdir(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  });
  // The CLI prints a new link's update link, which holds its key: keep that line out.
  for (const line of (uploaded.stderr ?? '').split('\n')) if (line && !line.includes('/manage#')) say(`  appdrop: ${line}`);
  let result = null;
  try { result = JSON.parse(uploaded.stdout); } catch {}
  if (uploaded.status !== 0 || !parseLink(result?.url ?? '')) {
    return fail('upload_failed', `share: npx ${CLI} failed: ${result?.error ?? uploaded.error?.message ?? `exit ${uploaded.status}`}`);
  }
  const url = parseLink(result.url).url;
  writeFileSync(recordPath, `${JSON.stringify({ url, branch, source, revision: result.revision ?? revision + 1, version: shown, build, at: new Date().toISOString() }, null, 2)}\n`);
  rmSync(ipa, { force: true });
  const manifestPath = resolve(app.dir, 'app.json'), written = readFileSync(manifestPath, 'utf8');
  if (withBuildNumber(written, build) !== written) {
    writeFileSync(manifestPath, withBuildNumber(written, build));
    say(`share: app.json's buildNumber is ${build} now; commit it, alone or with the change this build ships.`);
  }
  say(`share: open this on an iPhone to install ${app.displayName ?? app.name}; it installs with that person's own Apple ID.`);
  console.log(json ? JSON.stringify({ url, revision: result.revision ?? revision + 1, version: shown, build, branch, action: step.action === 'create' ? 'created' : 'updated' }) : url);
  return 0;
}

if (import.meta.main) process.exit(await main(process.argv.slice(2)));
