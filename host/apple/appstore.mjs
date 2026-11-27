// The Mac App Store lane (LLP 1030.005): `exact appstore <app>` builds the store's `.pkg`. A production build
// sandboxed with the bake's entitlements (D2), linking no update store (D3), signed by the team's Apple Distribution
// identity under a Mac App Store profile, each nested part for its role (D1), wrapped by `productbuild` and signed
// with the Mac Installer Distribution identity, then checked here and validated with Apple where an App Store Connect
// key is configured. `--ad-hoc` builds the same configuration signed ad hoc, to launch on this Mac (D4).
//
// The verb stops at the checked `.pkg` and prints the upload command; uploading waits for the first submission by hand
// (§6 piece 7).
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { resolve } from 'node:path';
import { storeKeys } from './plist.mjs';
import { signRelease, signedEntitlements, PROFILE_KEYS } from './assets.mjs';
import { appleExecutableName } from '../../scripts/app.mjs';

export const APPLICATION_KINDS = ['Apple Distribution', '3rd Party Mac Developer Application'];
export const INSTALLER_KINDS = ['Mac Installer Distribution', '3rd Party Mac Developer Installer'];

/** The valid identities `security find-identity -v -p <policy>` lists, as `{ sha1, name, team }`. The installer
 *  certificate is listed under `basic` and not under `codesigning`. */
export function listIdentities(policy) {
  const out = spawnSync('security', ['find-identity', '-v', '-p', policy], { encoding: 'utf8' }).stdout ?? '';
  return [...out.matchAll(/^\s*\d+\)\s+([0-9A-F]{40})\s+"([^"]+)"/gm)]
    .map(([, sha1, name]) => ({ sha1, name, team: /\(([A-Z0-9]{10})\)$/.exec(name)?.[1] ?? null }));
}

/** One identity of `kinds` from `list`: the one `named` gives (a SHA-1 or certificate name), else the sole one of
 *  `team` (any team when null). `what` describes the certificate; `variable` names it. Returns `{ identity }` or
 *  `{ problem }`. */
function chooseIdentity(list, { kinds, named, team, what, variable }) {
  const of = (id) => kinds.some((kind) => id.name.startsWith(`${kind}: `));
  if (named) {
    const id = list.find((i) => i.sha1 === named.toUpperCase() || i.name === named);
    if (!id) return { problem: `${variable}=${named} names no valid identity in the keychain` };
    if (!of(id)) return { problem: `${variable} names "${id.name}", which is not ${what}` };
    return { identity: id };
  }
  const fitting = [...new Map(list.filter((i) => of(i) && (!team || i.team === team)).map((i) => [i.sha1, i])).values()];
  if (fitting.length === 1) return { identity: fitting[0] };
  if (fitting.length > 1) return { problem: `several ${kinds[0]} identities are on this Mac (${fitting.map((i) => `"${i.name}"`).join(', ')}); name one with ${variable}=<sha1>` };
  return { problem: null };
}

/** Apple's key directories, which `altool` searches for `AuthKey_<key id>.p8` (`xcrun altool --help`, "--api-key"). */
export const keyDirectories = (env = process.env, home = homedir(), cwd = process.cwd()) => [env.API_PRIVATE_KEYS_DIR,
  resolve(cwd, 'private_keys'), resolve(home, 'private_keys'), resolve(home, '.private_keys'), resolve(home, '.appstoreconnect/private_keys')].filter(Boolean);

/** Apple's validation of the `.pkg` (D1 item 6): `{ keyId, issuer, key }` when an App Store Connect API key is
 *  configured (LLP 1030.003 D6's names); `{ skip }` when neither name is set; `{ problem }` when one is and the other,
 *  or the key file, is missing. Xcode's `altool` (27.10) validates a package with the key alone: the app's App Store
 *  Connect id is read from the package's bundle id. */
export function ascValidation(env = process.env, directories = keyDirectories(env)) {
  const names = ['EXACT_ASC_KEY_ID', 'EXACT_ASC_ISSUER_ID'];
  const where = `AuthKey_<key id>.p8 in one of ${directories.join(', ')}`;
  if (!names.some((n) => env[n])) return { skip: `Apple's validation was not run: set EXACT_ASC_KEY_ID and EXACT_ASC_ISSUER_ID (an App Store Connect API key), with ${where}` };
  const missing = names.filter((n) => !env[n]);
  const key = env.EXACT_ASC_KEY_ID ? directories.map((d) => resolve(d, `AuthKey_${env.EXACT_ASC_KEY_ID}.p8`)).find((p) => existsSync(p)) : null;
  if (env.EXACT_ASC_KEY_ID && !key) missing.push(`AuthKey_${env.EXACT_ASC_KEY_ID}.p8 (in one of ${directories.join(', ')})`);
  if (missing.length) return { problem: `Apple's validation needs ${missing.join(', ')} beside what is set` };
  return { keyId: env.EXACT_ASC_KEY_ID, issuer: env.EXACT_ASC_ISSUER_ID, key };
}

/** What the store build needs before anything compiles, every lack named at once: the category the store requires,
 *  and, unless `adHoc`, the signing and installer identities and a whole App Store Connect configuration or none.
 *  Returns `{ identity, installer, asc }` (identities `{ sha1, name, team }`); throws the refusals. */
export function preflight(app, { adHoc = false, env = process.env, list = listIdentities, directories = keyDirectories(env) } = {}) {
  const problems = [];
  if (!storeKeys(app, 'macos').LSApplicationCategoryType) problems.push('app.json names no category Apple files apps under, and the Mac App Store requires LSApplicationCategoryType: add one to "categories", such as "productivity", "utilities" or "developer-tools"');
  if (adHoc) {
    if (problems.length) throw new Error(problems.join('\n'));
    return { identity: null, installer: null, asc: { skip: 'an ad hoc build is not validated' } };
  }
  const team = env.DEVELOPMENT_TEAM?.trim() || null;
  const signing = chooseIdentity(list('codesigning'), { kinds: APPLICATION_KINDS, named: env.EXACT_APPSTORE_ID, team, what: 'an Apple Distribution (or 3rd Party Mac Developer Application) certificate', variable: 'EXACT_APPSTORE_ID' });
  if (signing.problem) problems.push(signing.problem);
  else if (!signing.identity) problems.push('no "Apple Distribution" certificate is on this Mac (the older form is "3rd Party Mac Developer Application"), and the Mac App Store takes only a build signed with one. Make it in the Apple Developer account (Certificates), open it once so it lands in the login keychain, or name one with EXACT_APPSTORE_ID=<sha1>');
  const installing = chooseIdentity(list('basic'), { kinds: INSTALLER_KINDS, named: env.EXACT_APPSTORE_INSTALLER_ID, team: signing.identity?.team ?? team, what: 'a Mac Installer Distribution (or 3rd Party Mac Developer Installer) certificate', variable: 'EXACT_APPSTORE_INSTALLER_ID' });
  if (installing.problem) problems.push(installing.problem);
  else if (!installing.identity) problems.push(`no "Mac Installer Distribution" certificate${signing.identity?.team ? ` of team ${signing.identity.team}` : ''} is on this Mac (the older form is "3rd Party Mac Developer Installer"), and productbuild signs the store's .pkg with one. Make it in the Apple Developer account and open it once, or name one with EXACT_APPSTORE_INSTALLER_ID=<sha1>; security find-identity -p basic lists it, and -p codesigning does not`);
  const asc = ascValidation(env, directories);
  if (asc.problem) problems.push(asc.problem);
  if (problems.length) throw new Error(problems.join('\n'));
  return { identity: signing.identity, installer: installing.identity, asc };
}

/** Each signature in `order` that carries `get-task-allow`, which no store build may (D1 item 4); `signed` is
 *  `signedEntitlements(order)`. */
export const debuggable = (signed) => signed.filter(({ entitlements }) => entitlements['com.apple.security.get-task-allow']).map(({ path }) => path);

/** The app's signed entitlements, each beside what derived it (D1 item 6): the bake's `sandboxSources`, the profile,
 *  a device grant's hardened-runtime key, or the build's own groups. One line each. */
export function entitlementLines(entitlements, compat) {
  const sources = compat?.reach?.sandboxSources ?? {};
  const device = new Set(compat?.reach?.entitlements ?? []);
  return Object.keys(entitlements).sort().map((key) => `  ${key}: ${sources[key]?.join('; ')
    ?? (PROFILE_KEYS.includes(key) ? 'the provisioning profile' : device.has(key) ? 'a device grant (reach.entitlements)'
      : key === 'com.apple.security.application-groups' ? 'the extensions\' App Group' : 'the build')}`);
}

/** `exact appstore <app>` (D1), or with `adHoc` the store configuration signed ad hoc (D4). `build(app, options)` runs
 *  the store build and returns its bundle; `out` is the folder the artifacts go in. Returns the `.pkg` (none ad hoc). */
export function appstore(app, { adHoc = false, build, out, env = process.env, log = console.log, list = listIdentities, sh = run, sign = signRelease, signed: readSigned = signedEntitlements } = {}) {
  const { identity, installer, asc } = preflight(app, { adHoc, env, list });
  if (app.manifest.deploy?.store?.macos !== '0') log(`deploy.store.macos is "${app.manifest.deploy?.store?.macos ?? 'A'}" for the Developer ID build; the store build bakes "0": it links no update store, and every change is a new store submission (LLP 1030.005 D3).`);
  log(`version ${env.EXACT_VERSION ?? '0.1.0'} (build ${env.EXACT_BUILD_NUMBER ?? '1'}), from EXACT_VERSION and EXACT_BUILD_NUMBER; each upload needs a higher build number.`);
  const sha1 = adHoc ? '-' : identity.sha1;
  const bundle = build(app, { adHoc, identity: sha1 });
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const staged = resolve(out, bundle.split('/').pop());
  sh('/usr/bin/ditto', [bundle, staged]);
  const compat = JSON.parse(readFileSync(resolve(staged, 'Contents/Resources/receipt.json'), 'utf8')).build?.compat;
  if (compat?.inputs?.store?.L !== '0') throw new Error('the store build baked an update store; rebuild with this checkout (LLP 1030.005 D3)');
  const order = sign(staged, { compat, manifest: app.manifest, identity: sha1, id: app.id, scratch: out, store: true,
    hardened: !adHoc, timestamp: adHoc ? ['--timestamp=none'] : ['--timestamp'] });
  sh('codesign', ['--verify', '--deep', '--strict', '--verbose=1', staged]);
  const signed = readSigned(order);
  const allowing = debuggable(signed);
  if (allowing.length) throw new Error(`get-task-allow is signed into ${allowing.join(', ')}; the store takes none (LLP 1030.005 D1)`);
  log(`signed entitlements of ${staged}:\n${entitlementLines(signed.at(-1).entitlements, compat).join('\n')}`);
  if (adHoc) {
    log(`${staged}\n  The store configuration signed ad hoc, without the hardened runtime (an ad hoc signature has no Team ID, so library validation would refuse Exact's own libraries) and without a profile, so a grant that needs one is unavailable. It proves the sandbox, the entitlements and the missing update store, not the signature, which only TestFlight proves. A production build ignores the agent: open it by hand.`);
    return null;
  }
  const pkg = resolve(out, `${appleExecutableName(app)}.pkg`);
  sh('productbuild', ['--component', staged, '/Applications', '--sign', installer.sha1, pkg]);
  sh('pkgutil', ['--check-signature', pkg]);
  if (asc.skip) log(asc.skip);
  else sh('xcrun', ['altool', '--validate-app', pkg, '--api-key', asc.keyId, '--api-issuer', asc.issuer, '--p8-file-path', asc.key]);
  log(`\n${pkg}\nUpload it with Transporter, or: xcrun altool --upload-package ${pkg} --api-key <key id> --api-issuer <issuer id>`);
  return pkg;
}

/** A tool the verb depends on, run to completion: half a signed package is worse than none. */
function run(cmd, args) {
  const r = spawnSync(cmd, args, { stdio: 'inherit' });
  if (r.status !== 0) throw new Error(`${cmd} ${args[0]} failed (exit ${r.status ?? 'signal'})`);
}
