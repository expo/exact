// What a Mac build signs that only a provisioning profile authorizes (Apple
// TN3125; LLP 1133.010 D7): the device rows' Mac entitlements (push, App
// Attest, the Declared Age Range, Group Activities, the iCloud key-value
// store, CloudKit, Sensitive Content Analysis), the keychain group a locked secret is kept
// in (a Mac keeps one only in the data-protection keychain, whose access
// groups the signature names, TN3137), passkeys' and HTTPS auth callbacks'
// associated domains, and Sign in with Apple's and Apple Pay's entitlements
// (LLP 1030.005 §6 piece 4).
// The build picks a macOS profile for the app that allows them all, signs
// with that profile's team's identity and embeds it. With none, a
// development build signs none of them and says what each feature then
// reports; a release build with a line it cannot keep is refused.
import { installedProfiles, decodeProfile, profilesFor, byExpiry, signedEnvironment, allowsEntitlement, profileMisfit, macHardwareID, METHODS, identity, profileIdentity, developmentIdentities } from './devices.mjs';
import { widgetSigningTeam, macWidgetProfiles, macWidgetSigning, platformWidgetEntries, macWidgetGroup, kindPhrase, identityVariable } from './widgets.mjs';
import { shareId } from './extensions.mjs';
import { appleLinkDomains, webAuthCallback } from '../../scripts/links.mjs';
import { appClip } from '../../scripts/app.mjs';

const POLICIES = ['user-presence', 'biometry-any', 'biometry-current-set', 'device-credential'];

/** The grant lines that keep a name under a presence policy: `{ name, optional }` each. */
export function presenceLines(ceiling = '') {
  return String(ceiling).split('\n').map((line) => line.trim().split(/\s+/))
    .filter((w) => w[0] === 'secret.keep' && POLICIES.includes(w[2]))
    .map((w) => ({ name: w[1], optional: w[3] === 'optional' }));
}

const KEYCHAIN = 'keychain-access-groups', DOMAINS = 'com.apple.developer.associated-domains';
const SIGN_IN = 'com.apple.developer.applesignin', PAY = 'com.apple.developer.in-app-payments';
/** `device.icloud-kv`'s entitlement, valued as the application identifier is: `<prefix>.<bundle id>`. */
export const KEY_VALUE_STORE = 'com.apple.developer.ubiquity-kvstore-identifier';
const CLOUDKIT_SERVICES = 'com.apple.developer.icloud-services';
const CLOUDKIT_CONTAINERS = 'com.apple.developer.icloud-container-identifiers';
const CLOUDKIT_ENVIRONMENT = 'com.apple.developer.icloud-container-environment';
/** Each device row's Mac profile entitlement (`reach.profileEntitlements`, runner/src/device.rs): its line, what the
 * app reports while it is not signed, and whether a release without it is refused. A Mac release without push ships,
 * reporting it unavailable, as Android and the web do (LLP 1133.004 §9 P7), and so does one without SharePlay, whose
 * package is not exact2's to name, or CloudKit (LLP 1133.001.000 D3); App Attest and the age range are refused (LLP 1133.028 D17),
 * as are the iCloud key-value store and Sensitive Content Analysis, which fail without saying so. */
const ROW_EFFECTS = {
  'com.apple.developer.aps-environment': ['device.push', 'pushPermissionState().available is false and subscribePush fails NotSupportedError', false],
  'com.apple.developer.devicecheck.appattest-environment': ['device.attestation', 'attestationStatus().available is false and attest fails NotSupportedError', true],
  'com.apple.developer.declared-age-range': ['device.age-range', 'ageRangeStatus().available is false and requestAgeRange fails NotSupportedError', true],
  'com.apple.developer.group-session': ['device.shareplay', 'SharePlay is unavailable to the app', false],
  ...Object.fromEntries([CLOUDKIT_SERVICES, CLOUDKIT_CONTAINERS, CLOUDKIT_ENVIRONMENT]
    .map(key => [key, ['device.cloudkit', 'CloudKit is unavailable to the app', false]])),
  // Unsigned, the store keeps values on this Mac without saying so, so a release that cannot sign it is refused.
  [KEY_VALUE_STORE]: ['device.icloud-kv', 'NSUbiquitousKeyValueStore keeps its values on this Mac and syncs none', true],
  // Unsigned, the analyzer finds nothing sensitive (Apple, "com.apple.developer.sensitivecontentanalysis.client").
  'com.apple.developer.sensitivecontentanalysis.client': ['device.sensitive-content-analysis', 'SCSensitivityAnalyzer reports nothing as sensitive', true],
};

/** Whether a build signs device grant `line` (`device.<row>`): the bake's ceiling grants it, and on the Mac
 * (`provisioning` given, null where no profile was chosen), each of the row's entitlements a profile authorizes was
 * signed under the one chosen. An iOS device build that cannot sign a granted row is refused, so there the grant
 * settles it. */
export function signsGrant(line, compat, provisioning) {
  const granted = String(compat?.inputs?.grantCeiling ?? '').split('\n').some((l) => l.trim().split(/\s+/)[0] === line);
  if (!granted || provisioning === undefined) return granted;
  return Object.entries(ROW_EFFECTS).filter(([, [row]]) => row === line).every(([key]) => provisioning?.entitlements?.[key] !== undefined);
}

/** What a macOS build needs a profile for, from its bake (`compat`): `{ entitlement, lines, effect, mandatory }`
 * each, where a mandatory one refuses a release that cannot sign it. */
export function macProfileNeeds(compat, app = null) {
  const ceiling = compat?.inputs?.grantCeiling ?? '', needs = [];
  for (const name of compat?.reach?.profileEntitlements ?? []) {
    const [lines, effect, mandatory] = ROW_EFFECTS[name] ?? [name, `${name} is not signed`, true];
    needs.push({ entitlement: name, lines, effect, mandatory });
  }
  const kept = presenceLines(ceiling);
  if (kept.length) {
    const mandatory = kept.filter((l) => !l.optional).map((l) => `secret.keep ${l.name}`);
    needs.push({ entitlement: KEYCHAIN, lines: kept.map((l) => `secret.keep ${l.name}`).join(', '), mandatory: mandatory.length > 0,
      effect: `localAuthStatus().presenceSecrets is false${mandatory.length ? `, a set of ${mandatory.join(', ')} throws NotSupportedError` : ''}${mandatory.length < kept.length ? ', an optional name stays plain and protect() rejects' : ''}` });
  }
  const passkeys = passkeyLines(ceiling);
  if (passkeys.length) needs.push({ entitlement: DOMAINS, lines: passkeys.map((rp) => `auth.passkey ${rp}`).join(', '), mandatory: true,
    effect: 'getClientCapabilities().available is false and create and get fail NotSupportedError' });
  const callbacks = (compat?.reach?.auth?.callbacks ?? []).filter((callback) => {
    const url = new URL(callback);
    return url.protocol === 'https:' && !webAuthCallback(url);
  });
  if (callbacks.length) needs.push({ entitlement: DOMAINS, lines: callbacks.map((callback) => `auth.callback ${callback}`).join(', '), mandatory: true,
    effect: 'HTTPS auth sessions fail association validation' });
  // Verified links (LLP 1038 D8) are not mandatory: unsigned, they open in the browser.
  const links = app?.manifest ? appleLinkDomains(app.manifest) : [];
  if (links.length) needs.push({ entitlement: DOMAINS, lines: 'handle_links "preferred"', mandatory: false,
    effect: `links to ${links.map((d) => d.slice('applinks:'.length)).join(', ')} open in the browser` });
  // Both report unavailable unsigned (packages/sign-in, packages/payments), so a release without them ships.
  const clients = compat?.reach?.auth?.signIn?.apple ?? [];
  if (clients.length) needs.push({ entitlement: SIGN_IN, lines: clients.map((id) => `auth.apple ${id}`).join(', '), mandatory: false,
    effect: 'getIdentityCapabilities() reports Apple inactive and an Apple identity request fails NotSupportedError' });
  const merchants = compat?.reach?.payments?.merchantIds ?? [];
  if (merchants.length) needs.push({ entitlement: PAY, lines: merchants.map((id) => `payment.apple-pay ${id}`).join(', '), mandatory: false,
    effect: 'Apple Pay reads unavailable: canMakePayment() is false' });
  return needs;
}

/** The bake's entitlement values, with CloudKit's default container derived from the bundle id at signing. */
export function grantEntitlementValues(app, reach) {
  return { [CLOUDKIT_CONTAINERS]: [`iCloud.${app.id}`], ...reach?.entitlementValues };
}

/** Whether macOS profile `p` allows `name` for `app`: its keychain group, push's environment, or the
 * entitlement's value when the bake gives one. CloudKit's environment follows the signature. */
export function macAllows(p, app, name, values = {}) {
  if (name === KEYCHAIN) return p.keychain.includes(`${p.prefix}.${app.id}`) || p.keychain.includes(`${p.prefix}.*`);
  if (name === 'com.apple.developer.aps-environment') return allowsEntitlement(p, 'aps-environment');
  return allowsEntitlement(p, name, values);
}

const signingChoice = method => method === 'developerid' || method === 'appstore'
  ? `set ${identityVariable(true, method === 'appstore' ? method : null)}=<sha1> to choose ${kindPhrase(true, method === 'appstore' ? method : null)} identity`
  : 'set DEVELOPMENT_TEAM=<team-id> or EXACT_IDENTITY=<sha1> to choose an Apple Development identity, or EXACT_IDENTITY=- to sign ad hoc (drops Mac extensions, leaves push, App Attest, age range, SharePlay, keychain-access groups, passkeys, HTTPS auth callbacks, Sign in with Apple and Apple Pay unsigned, and makes stored login-keychain items prompt after rebuilds)';
const teamSelectedByEnvironment = (team, method) => method === 'development' && !process.env.EXACT_IDENTITY && team === process.env.DEVELOPMENT_TEAM?.trim();

/** The macOS profile a build of `app` signs `required` under: for its id or the team's wildcard, unexpired, of the
 * build's `method` (`development`, naming this Mac; `developerid`; `appstore`), allowing all of `required`; the
 * longest-lived within DEVELOPMENT_TEAM, or the sole fitting team. EXACT_MAC_PROFILE overrides the team and must
 * fit. Several teams are refused while the identity is unresolved; only DEVELOPMENT_TEAM requires a suitable profile. */
export function macProfile(app, required, { method = 'development', installed = installedProfiles(),
  named = process.env.EXACT_MAC_PROFILE ? decodeProfile(process.env.EXACT_MAC_PROFILE) : null, udid,
  team = process.env.DEVELOPMENT_TEAM?.trim(), accept = () => true, values = {} } = {}) {
  const mac = (p) => p.platforms?.includes('OSX');
  const lacking = (p) => required.filter((name) => !macAllows(p, app, name, values));
  const misfit = (p) => !mac(p) ? `is for ${p.platforms?.join(', ') || 'another platform'}, not macOS`
    : !profilesFor(app.id, [p]).length ? `is for ${p.appId}, not ${app.id}`
    : p.method !== method ? `is ${METHODS[p.method]}, not ${METHODS[method]}`
    : profileMisfit(p, method === 'development' ? (udid ??= macHardwareID()) : null)?.replace('this phone', 'this Mac')
    ?? (lacking(p).length ? `does not allow ${lacking(p).join(', ')}` : !accept(p) ? 'does not cover the Mac extensions and their shared groups' : null);
  if (named) {
    const why = misfit(named);
    if (why) throw new Error(`host/apple: EXACT_MAC_PROFILE (${named.name}) ${why}`);
    return named;
  }
  const fitting = installed.filter((p) => (!team || p.team === team) && mac(p) && !misfit(p)).sort(byExpiry);
  if (team && !fitting.length && teamSelectedByEnvironment(team, method)) throw new Error(`host/apple: no suitable ${METHODS[method]} macOS provisioning profile for ${app.id} of team ${team} (DEVELOPMENT_TEAM); install one allowing ${required.join(', ') || 'the app and its extensions'}, unset DEVELOPMENT_TEAM, or name a profile with EXACT_MAC_PROFILE`);
  const teams = [...new Set(fitting.map(p => p.team))];
  if (teams.length > 1) throw new Error(`host/apple: macOS profiles for ${app.id} fit several teams (${teams.join(', ')}); ${signingChoice(method)}`);
  return fitting[0] ?? null;
}

/** The entitlements `profile` signs `app` with for `needs`: its application identifier and team, each row's (an
 * environment as the profile signs it, LLP 1133.028 D15 item 1; a value the bake gives, `reach.entitlementValues`;
 * otherwise true), the app's own keychain group, the bake's
 * associated domains with `handle_links`' `applinks:` (LLP 1038 D8), Sign in with Apple's `Default` and the bake's
 * merchant IDs. */
export function macProfileEntitlements(app, profile, needs, compat) {
  const out = { 'com.apple.application-identifier': `${profile.prefix}.${app.id}`, 'com.apple.developer.team-identifier': profile.team };
  const values = grantEntitlementValues(app, compat?.reach);
  for (const { entitlement } of needs) {
    if (entitlement === KEYCHAIN) out[KEYCHAIN] = [`${profile.prefix}.${app.id}`];
    else if (entitlement === DOMAINS) out[DOMAINS] = [...new Set([...associatedDomains(compat?.reach?.auth, profile.dev), ...(app.manifest ? appleLinkDomains(app.manifest) : [])])];
    else if (entitlement === SIGN_IN) out[SIGN_IN] = ['Default'];
    else if (entitlement === PAY) out[PAY] = compat?.reach?.payments?.merchantIds ?? [];
    else if (entitlement === KEY_VALUE_STORE) out[KEY_VALUE_STORE] = out['com.apple.application-identifier'];
    else out[entitlement] = values[entitlement] ?? entitlementValue(entitlement, profile.dev);
  }
  return out;
}

/** The provisioning a macOS build of `app` signs with (LLP 1133.010 D7): `{ profile, entitlements }`, or null where
 * nothing needs a profile or none fits. Without one, a development build says what it leaves unsigned and what the
 * app then reports, and a release (`release`) with a mandatory need is refused (`grant-needs-profile`). `method` is
 * the profile's kind: `development`, or `developerid` for a Developer ID release. `reason`: why no profile can sign,
 * where it is not that none fits. */
export function macProvisioning(app, compat, { release = false, method = 'development', log = console.log, find = macProfile, reason = null, beside = '' } = {}) {
  const needs = macProfileNeeds(compat, app);
  // A store build always embeds its profile: TestFlight requires one, whatever TN3125 lets an App Store build omit (LLP 1030.005 D1).
  const always = method === 'appstore' && !reason;
  if (!needs.length && !always) return null;
  const required = [...new Set(needs.map((n) => n.entitlement))];
  const profile = reason ? null : find(app, required, { method, values: grantEntitlementValues(app, compat?.reach) });
  if (profile) {
    log(`macOS profile: "${profile.name}" (team ${profile.team}, ${METHODS[profile.method]}) signs ${required.join(', ')} for ${needs.map((n) => n.lines).join(', ')}.`);
    return { profile, entitlements: macProfileEntitlements(app, profile, needs, compat) };
  }
  const why = reason ?? `no ${METHODS[method]} macOS provisioning profile on this Mac for ${app.id}${method === 'development' ? ' that names this Mac' : ''}${required.length ? ` allows ${required.join(', ')}` : ''}${beside}; in the Apple Developer portal, ${required.length ? `enable those capabilities for the App ID ${app.id}, ` : ''}make ${method === 'development' ? 'a' : 'an'} ${METHODS[method]} macOS profile for it${method === 'development' ? ' that includes this Mac' : ''} and install it, or name one with EXACT_MAC_PROFILE (LLP 1133.010 D7)`;
  if (always && release) throw new Error(`appstore-needs-profile: ${why}. TestFlight requires the profile (LLP 1030.005 D1).`);
  const refused = needs.filter((n) => n.mandatory);
  if (release && refused.length) throw new Error(`grant-needs-profile: ${refused.map((n) => n.lines).join(', ')}: ${why}.${refused.some((n) => n.entitlement === KEYCHAIN) ? ' Or write `optional` on each secret.keep line.' : ''}`);
  for (const n of needs) log(`${n.lines}: this build signs no ${n.entitlement}, since ${why}: ${n.effect}.`);
  return null;
}

/** The team of the signing identity `sha1`, as its certificate says, for the build's kind (`signingKinds`); null when
 * it is not one. */
const certificateTeam = (sha1, distribution, store = null) => { try { return widgetSigningTeam(sha1, { distribution, store }); } catch { return null; } };

/** The identity that signs under `profile`: one of the profile's team, or the system refuses the entitlements at
 * launch. A development build takes the team's Apple Development identity (`find`; EXACT_IDENTITY names one, whose
 * certificate must be of that kind and team); a Developer ID build keeps `current` (EXACT_DEVELOPER_ID), and a Mac App
 * Store build (`store`) its Apple Distribution identity (EXACT_APPSTORE_ID), each refused when another team's. */
export function macProfileIdentity(profile, current, { distribution = false, store = null, find = identity, teamOf = certificateTeam, profiles = [] } = {}) {
  const authorizing = [profile, ...profiles], variable = identityVariable(distribution, store);
  const sha1 = distribution ? current : find(profile.team, { profiles: authorizing });
  if (!sha1 || sha1 === '-' || teamOf(sha1, distribution, store) !== profile.team) {
    throw new Error(distribution
      ? `host/apple: the ${store ? 'App Store' : 'Developer ID'} profile "${profile.name}" is team ${profile.team}'s; sign with that team's ${kindPhrase(distribution, store).replace(/^an? /, '')} identity (${variable}=<sha1>)`
      : `host/apple: the macOS profile "${profile.name}" is team ${profile.team}'s, and ${sha1} is not that team's Apple Development identity; name one with EXACT_IDENTITY=<sha1> (security find-identity -v -p codesigning)`);
  }
  return distribution ? profileIdentity(sha1, authorizing, { selector: variable }) : sha1;
}

/** The App Sandbox entitlements a sandboxed build signs the app with (LLP 1030.005 D2): the bake's
 * `reach.sandboxEntitlements`, each true. Any of `reach.sandboxRefusals` refuses the build, all named at once. */
export function sandboxEntitlements(compat) {
  const refusals = compat?.reach?.sandboxRefusals ?? [];
  if (refusals.length) throw new Error(`grant-sandbox: the Mac App Store sandbox cannot take ${refusals.length === 1 ? 'this line' : 'these lines'} (LLP 1030.005 D2); a Developer ID build keeps them:\n  ${refusals.join('\n  ')}`);
  const keys = compat?.reach?.sandboxEntitlements;
  if (!keys) throw new Error('grant-sandbox: the bake reports no sandbox entitlements; rebuild with this checkout');
  return Object.fromEntries(keys.map((key) => [key, true]));
}

/** The App Store profile a store build's share extension embeds (ITMS-90889: TestFlight asks one of every bundle): for
 * `<id>.share`, of `team`, allowing its App Group (`group`) or the team's. Null when none is installed. */
export function shareStoreProfile(app, team, { installed, find = macProfile } = {}) {
  const group = macWidgetGroup(app, team);
  return find({ id: shareId(app) }, [], { method: 'appstore', team, named: null, ...(installed ? { installed } : {}),
    accept: (p) => p.groups.includes(group) || p.groups.includes(`${team}.*`) });
}

/** How a macOS build of `app` signs, once its bake (`compat`) says what its grants need, resolved together:
 * `{ provisioning, widgets, sha1, sandbox?, shareTeam?, shareProfile? }`. A null identity is resolved from fitting
 * profiles before deciding which extensions to build. With widgets one profile serves both their shared groups and the
 * grants. Explicit ad hoc signing (EXACT_IDENTITY=-) omits extensions and signs no profile's entitlements. */
export function macSigning(app, compat, { current, widgets = null, release = false, distribution = false, store = null, sandboxed = false, log = console.log,
  find = macProfile, widgetProfiles = macWidgetProfiles, identityFor = macProfileIdentity, teamOf = certificateTeam,
  signWidgets = macWidgetSigning, namedWidgetProfile, identities = developmentIdentities, adHoc = current === '-' && (!distribution || !!store),
  shareProfile = shareStoreProfile } = {}) {
  const wantsWidgets = app.manifest && platformWidgetEntries(app, 'macos').length > 0;
  const wantsShare = !!app.manifest?.share_target;
  const method = store ?? (distribution ? 'developerid' : 'development');
  const values = grantEntitlementValues(app, compat?.reach);
  // A store build signs the App Sandbox, as `mac --sandbox` does with the dev server's connection besides (LLP 1030.005
  // D4); a line the sandbox cannot take refuses either before any profile is looked for.
  const sandbox = store || sandboxed ? { ...sandboxEntitlements(compat), ...(store ? {} : { 'com.apple.security.network.client': true }) } : null;
  const kind = { distribution, store };
  const teamOfKind = (sha1) => teamOf(sha1, distribution, store);
  const ambiguous = !current && !distribution && new Set(identities().map(id => id.team)).size > 1;
  // Ordinary kept secrets also need a stable certificate signature across rebuilds (LLP 1018 D7).
  if (!current && (ambiguous || wantsWidgets || wantsShare)) {
    const required = [...new Set(macProfileNeeds(compat, app).map(n => n.entitlement))];
    if (wantsWidgets && namedWidgetProfile === undefined) namedWidgetProfile = process.env.EXACT_PROFILE ? decodeProfile(process.env.EXACT_PROFILE) : null;
    const named = wantsWidgets && namedWidgetProfile?.platforms?.includes('OSX')
      ? widgetProfiles({ app, team: namedWidgetProfile.team, ...kind, log, named: namedWidgetProfile,
        also: p => required.every(name => macAllows(p, app, name, values)) }) : null;
    const profile = find(app, required, { method, values, ...(named ? { named: named.app } : {}),
      accept: p => !wantsWidgets || (named ? p.path === named.app.path : !!widgetProfiles({ app, team: p.team, ...kind, log,
        also: owner => owner.path === p.path && required.every(name => macAllows(owner, app, name, values)) })) });
    if (!profile && ambiguous) throw new Error(`host/apple: Apple Development identities span several teams, and no installed macOS profile selects a team for ${app.id}; ${signingChoice(method)}`);
    if (!profile) throw new Error(`host/apple: Mac extensions for ${app.id} need a signing team; no installed macOS profile covers their requirements. ${distribution ? `Set ${identityVariable(distribution, store)}=<sha1> to ${kindPhrase(distribution, store)} identity` : 'Set DEVELOPMENT_TEAM=<team-id> or EXACT_IDENTITY=<sha1> to choose an identity, or EXACT_IDENTITY=- to build without the extensions'}`);
    current = identityFor(profile, current, kind);
  }
  if (!widgets && wantsWidgets) widgets = signWidgets(app, current, { ...kind, log });
  const shareTeam = wantsShare && !adHoc ? (widgets?.team ?? teamOfKind(current)) : null;
  if (wantsShare && !adHoc && !shareTeam) throw new Error(`host/apple: the Mac share extension needs ${distribution ? `${kindPhrase(distribution, store)} identity; set ${identityVariable(distribution, store)}=<sha1>` : 'an Apple Development identity; set DEVELOPMENT_TEAM=<team-id> or EXACT_IDENTITY=<sha1>, or EXACT_IDENTITY=- to build without the extension'}`);
  // A store build's share extension is provisioned on its own (LLP 1030.005 D1).
  const shared = store && shareTeam ? shareProfile(app, shareTeam) : null;
  if (store && shareTeam && !shared && release) throw new Error(`appstore-needs-profile: no App Store macOS provisioning profile on this Mac for ${shareId(app)} of team ${shareTeam} allows the App Group ${macWidgetGroup(app, shareTeam)}; make one for the share extension's App ID and install it (LLP 1030.005 D1)`);
  const result = (provisioning, widgets, sha1) => ({ provisioning, widgets, sha1, ...(sandbox ? { sandbox } : {}),
    ...(wantsShare ? { shareTeam: adHoc ? null : widgets?.team ?? teamOfKind(sha1), ...(shared ? { shareProfile: shared } : {}) } : {}) });
  let pair = null;
  const together = (a, required) => {
    pair = widgetProfiles({ app: a, team: widgets.team, ...kind, log, also: (p) => required.every((name) => macAllows(p, a, name, values)) });
    if (!pair && teamSelectedByEnvironment(widgets.team, method)) throw new Error(`host/apple: no suitable macOS profile of team ${widgets.team} (DEVELOPMENT_TEAM) allows ${required.join(', ')} and the Mac widgets' shared groups; install matching app and extension profiles or name the app's with EXACT_PROFILE`);
    return pair?.app ?? null;
  };
  const reason = adHoc ? `this build is signed ad hoc (${store ? 'exact appstore --ad-hoc' : 'EXACT_IDENTITY=-'}), and no profile\'s entitlements hold under an ad hoc signature (LLP 1133.010 D7)` : null;
  const selectedTeam = widgets?.team ?? shareTeam ?? (current && current !== '-' ? teamOfKind(current) : null);
  const provisioning = macProvisioning(app, compat, { release, method, log, reason,
    beside: widgets ? `, with the Mac widgets' App Group and shared Keychain group for team ${widgets.team} (one profile signs both)` : '',
    find: widgets ? together : (a, required, options) => find(a, required, { ...options, ...(selectedTeam ? { team: selectedTeam } : {}) }) });
  const widgetPair = pair ?? widgets?.profiles;
  if (widgetPair) current = identityFor(widgetPair.app, current, { ...kind, profiles: [widgetPair.extension] });
  if (!provisioning) return result(provisioning, widgets, current);
  if (widgets) return result(provisioning, { ...widgets, profiles: pair }, current);
  return result(provisioning, widgets, identityFor(provisioning.profile, current, kind));
}
/** The Mac sample host's bundle identifier and name (LLP 1031 D10), signed under `profile` or none: under one, the
 * app's id, the App ID the profile grants its entitlements to (Apple ties them to it), so its sessions keep what the
 * app's do; with none, iOS's sample host's own second identity, `<id>.host`. */
export function macHostIdentity(app, profile) {
  return { id: profile ? app.id : `${app.id}.host`, name: 'Host (not Exact)' };
}

/** The RP IDs a Mac needs an associated domain for: every `auth.passkey` line but `localhost`, which only the web admits. */
export function passkeyLines(ceiling = '') {
  return String(ceiling).split('\n').map((line) => line.trim().split(/\s+/))
    .filter((w) => w[0] === 'auth.passkey' && w[1] && w[1] !== 'localhost').map((w) => w[1]);
}

/** A debuggable build appends `?mode=developer` to the bake's `webcredentials:` entries, so Apple fetches the
 * association from the domain rather than its CDN (LLP 1133.011 D6). Release builds keep the entries unsuffixed. */
export function associatedDomains(auth, debuggable) {
  return (auth?.associatedDomains ?? []).map((d) => debuggable && d.startsWith('webcredentials:') ? `${d}?mode=developer` : d);
}

/** Sign in with Apple's entitlement, which follows the signing, not the grant (LLP 1133.014 D7, D8): a simulator
 * takes it, and a device build takes it when its profile's entitlements (`allows`) include it. A development profile
 * and a distribution one carry it alike, when the App ID has the capability. Elsewhere it is left out, since an
 * entitlement the profile does not authorize stops the app installing. */
export function signInEntitlement(auth, { simulator = false, team = null, allows = [] } = {}) {
  const signable = (simulator && !team) || allows.includes('com.apple.developer.applesignin');
  return (auth?.signIn?.apple ?? []).length && signable ? { 'com.apple.developer.applesignin': ['Default'] } : {};
}

const CLIP_IDENTIFIERS = 'com.apple.developer.associated-appclip-app-identifiers';

/** The App Clip's application identifier, when the app's signing profile `p` lets the app name its clip, as App Store
 * Connect asks (ITMS-90876). Only a profile made after the clip's App ID was registered lists it. */
export function namedClip(app, p) {
  const clip = appClip(app), id = clip && p ? `${p.prefix}.${clip.id}` : null;
  return id && allowsEntitlement(p, CLIP_IDENTIFIERS, { [CLIP_IDENTIFIERS]: [id] }) ? id : null;
}

/** The entitlements that pair an app with its App Clip. The clip (`clipParent`, its parent's id) names its parent
 * under the App ID prefix `prefix` and signs on-demand install, which makes the bundle an App Clip: App Store Connect
 * refuses a clip without it (ITMS-90824). The app names its clip where its signing profile allows (`namedClip`). */
export function clipEntitlements(app, { prefix, profile = null } = {}) {
  if (app.clipParent) return { 'com.apple.developer.parent-application-identifiers': [`${prefix}.${app.clipParent}`], 'com.apple.developer.on-demand-install-capable': true };
  const id = !app.widgetTV && !app.widgetHost && !app.widgetAppId && namedClip(app, profile);
  return id ? { [CLIP_IDENTIFIERS]: [id] } : {};
}

/** A grant's entitlement's value (`reach.entitlements`): `true`, the bundle's `identifier` (its
 * `application-identifier`) for the iCloud key-value store, or the string the build chooses (LLP 1133.028 D15
 * item 1). An environment follows the signing, not the compiler: `debuggable` is the signing profile's
 * `get-task-allow`, true on a simulator, so `development` there and under a development profile and `production`
 * under the others. App Attest's and push's (`aps-environment`, LLP 1133.004 §9 P2, which a Live Activity that takes
 * pushes signs too); CloudKit uses `Development` and `Production`. Apple uses production for TestFlight and
 * App Store builds whatever they say. */
export function entitlementValue(name, debuggable, identifier = null) {
  if (name.endsWith('-environment')) return signedEnvironment(debuggable, name);
  if (name === KEY_VALUE_STORE) return identifier;
  return true;
}

/** The grant entitlements (`reach.entitlements`) a bundle of `app` signs, and so the ones its profile must allow:
 * all of them for the app; for an App Clip, whose notifications are ephemeral (`NSAppClip`), or an extension, which
 * reads no token, none of push's (LLP 1133.004 §9 P2), CloudKit's or the iCloud key-value store's, which are the app's.
 * The signature and the profile check both read this. */
export function grantEntitlements(app, names = []) {
  return app.clipParent || app.widgetAppId ? names.filter((name) => name !== 'aps-environment' && name !== KEY_VALUE_STORE && !name.startsWith('com.apple.developer.icloud-')) : names;
}

/** @ref LLP 1133.034 D13 — the `payment.apple-pay` grants' merchant IDs, where they can be signed: on a simulator, or
 * with a profile whose entitlements (`allows`) include Apple Pay's. Elsewhere (an ad hoc Mac build) Apple Pay is
 * unavailable, and a profile without it would refuse the build. */
export function merchantEntitlement(reach, { simulator = false, team = null, allows = [] } = {}) {
  const merchants = reach?.payments?.merchantIds ?? [];
  const signable = (simulator && !team) || allows.includes('com.apple.developer.in-app-payments');
  return merchants.length && signable ? { 'com.apple.developer.in-app-payments': merchants } : {};
}
