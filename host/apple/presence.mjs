// What a Mac build signs that only a provisioning profile authorizes (Apple
// TN3125; LLP 1133.010 D7): the device rows' Mac entitlements (push, App
// Attest, the Declared Age Range), the keychain group a locked secret is kept
// in (a Mac keeps one only in the data-protection keychain, whose access
// groups the signature names, TN3137), and a passkey's associated domains.
// The build picks a macOS profile for the app that allows them all, signs
// with that profile's team's identity and embeds it. With none, a
// development build signs none of them and says what each feature then
// reports; a release build with a line it cannot keep is refused.
import { installedProfiles, decodeProfile, profilesFor, byExpiry, signedEnvironment, allowsEntitlement, profileMisfit, macHardwareID, METHODS, identity } from './devices.mjs';
import { widgetSigningTeam, macWidgetProfiles } from './widgets.mjs';

const POLICIES = ['user-presence', 'biometry-any', 'biometry-current-set', 'device-credential'];

/** The grant lines that keep a name under a presence policy: `{ name, optional }` each. */
export function presenceLines(ceiling = '') {
  return String(ceiling).split('\n').map((line) => line.trim().split(/\s+/))
    .filter((w) => w[0] === 'secret.keep' && POLICIES.includes(w[2]))
    .map((w) => ({ name: w[1], optional: w[3] === 'optional' }));
}

const KEYCHAIN = 'keychain-access-groups', DOMAINS = 'com.apple.developer.associated-domains';
/** Each device row's Mac profile entitlement (`reach.profileEntitlements`, runner/src/device.rs): its line, what the
 * app reports while it is not signed, and whether a release without it is refused. A Mac release without push ships,
 * reporting it unavailable, as Android and the web do (LLP 1133.004 §9 P7); the other two are refused (LLP 1133.028
 * D17). */
const ROW_EFFECTS = {
  'com.apple.developer.aps-environment': ['device.push', 'pushPermissionState().available is false and subscribePush fails NotSupportedError', false],
  'com.apple.developer.devicecheck.appattest-environment': ['device.attestation', 'attestationStatus().available is false and attest fails NotSupportedError', true],
  'com.apple.developer.declared-age-range': ['device.age-range', 'ageRangeStatus().available is false and requestAgeRange fails NotSupportedError', true],
};

/** What a macOS build needs a profile for, from its bake (`compat`): `{ entitlement, lines, effect, mandatory }`
 * each, where a mandatory one refuses a release that cannot sign it. */
export function macProfileNeeds(compat) {
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
  return needs;
}

/** Whether macOS profile `p` allows `name` for `app`: the keychain group as `<prefix>.<id>` or the team's
 * `<prefix>.*`, push's environment as the build signs it under `p`, and any other key by name. */
export function macAllows(p, app, name) {
  if (name === KEYCHAIN) return p.keychain.includes(`${p.prefix}.${app.id}`) || p.keychain.includes(`${p.prefix}.*`);
  if (name === 'com.apple.developer.aps-environment') return allowsEntitlement(p, 'aps-environment');
  return p.entitlements.includes(name);
}

/** The macOS profile a build of `app` signs `required` under: for its id or the team's wildcard, unexpired, of the
 * build's `method` (`development`, naming this Mac; `developerid`; `appstore`), allowing all of `required`; the
 * longest-lived. EXACT_MAC_PROFILE names one, which must fit: a refusal says why it does not. Null when none fits. */
export function macProfile(app, required, { method = 'development', installed = installedProfiles(),
  named = process.env.EXACT_MAC_PROFILE ? decodeProfile(process.env.EXACT_MAC_PROFILE) : null, udid } = {}) {
  const mac = (p) => p.platforms?.includes('OSX');
  const lacking = (p) => required.filter((name) => !macAllows(p, app, name));
  const misfit = (p) => !mac(p) ? `is for ${p.platforms?.join(', ') || 'another platform'}, not macOS`
    : !profilesFor(app.id, [p]).length ? `is for ${p.appId}, not ${app.id}`
    : p.method !== method ? `is ${METHODS[p.method]}, not ${METHODS[method]}`
    : profileMisfit(p, method === 'development' ? (udid ??= macHardwareID()) : null)?.replace('this phone', 'this Mac')
    ?? (lacking(p).length ? `does not allow ${lacking(p).join(', ')}` : null);
  if (named) {
    const why = misfit(named);
    if (why) throw new Error(`host/apple: EXACT_MAC_PROFILE (${named.name}) ${why}`);
    return named;
  }
  return installed.filter((p) => mac(p) && !misfit(p)).sort(byExpiry)[0] ?? null;
}

/** The entitlements `profile` signs `app` with for `needs`: its application identifier and team, each row's (an
 * environment as the profile signs it, LLP 1133.028 D15 item 1), the app's own keychain group, and the bake's
 * associated domains. */
export function macProfileEntitlements(app, profile, needs, compat) {
  const out = { 'com.apple.application-identifier': `${profile.prefix}.${app.id}`, 'com.apple.developer.team-identifier': profile.team };
  for (const { entitlement } of needs) {
    if (entitlement === KEYCHAIN) out[KEYCHAIN] = [`${profile.prefix}.${app.id}`];
    else if (entitlement === DOMAINS) out[DOMAINS] = associatedDomains(compat?.reach?.auth, profile.dev);
    else out[entitlement] = entitlement.endsWith('-environment') ? signedEnvironment(profile.dev) : true;
  }
  return out;
}

/** The provisioning a macOS build of `app` signs with (LLP 1133.010 D7): `{ profile, entitlements }`, or null where
 * nothing needs a profile or none fits. Without one, a development build says what it leaves unsigned and what the
 * app then reports, and a release (`release`) with a mandatory need is refused (`grant-needs-profile`). `method` is
 * the profile's kind: `development`, or `developerid` for a Developer ID release. `reason`: why no profile can sign,
 * where it is not that none fits. */
export function macProvisioning(app, compat, { release = false, method = 'development', log = console.log, find = macProfile, reason = null, beside = '' } = {}) {
  const needs = macProfileNeeds(compat);
  if (!needs.length) return null;
  const required = [...new Set(needs.map((n) => n.entitlement))];
  const profile = reason ? null : find(app, required, { method });
  if (profile) {
    log(`macOS profile: "${profile.name}" (team ${profile.team}, ${METHODS[profile.method]}) signs ${required.join(', ')} for ${needs.map((n) => n.lines).join(', ')}.`);
    return { profile, entitlements: macProfileEntitlements(app, profile, needs, compat) };
  }
  const why = reason ?? `no ${METHODS[method]} macOS provisioning profile on this Mac for ${app.id}${method === 'development' ? ' that names this Mac' : ''} allows ${required.join(', ')}${beside}; in the Apple Developer portal, enable those capabilities for the App ID ${app.id}, make a ${METHODS[method]} macOS profile for it${method === 'development' ? ' that includes this Mac' : ''} and install it, or name one with EXACT_MAC_PROFILE (LLP 1133.010 D7)`;
  const refused = needs.filter((n) => n.mandatory);
  if (release && refused.length) throw new Error(`grant-needs-profile: ${refused.map((n) => n.lines).join(', ')}: ${why}.${refused.some((n) => n.entitlement === KEYCHAIN) ? ' Or write `optional` on each secret.keep line.' : ''}`);
  for (const n of needs) log(`${n.lines}: this build signs no ${n.entitlement}, since ${why}: ${n.effect}.`);
  return null;
}

/** The team of the signing identity `sha1`, as its certificate says, for the build's kind (Apple Development, or
 * Developer ID Application for `distribution`); null when it is not one. */
const certificateTeam = (sha1, distribution) => { try { return widgetSigningTeam(sha1, { distribution }); } catch { return null; } };

/** The identity that signs under `profile`: one of the profile's team, or the system refuses the entitlements at
 * launch. A development build takes the team's Apple Development identity (`find`; EXACT_IDENTITY names one, whose
 * certificate must be of that kind and team); a Developer ID build keeps `current` (EXACT_DEVELOPER_ID), refused when
 * it is another team's. */
export function macProfileIdentity(profile, current, { distribution = false, find = identity, teamOf = certificateTeam } = {}) {
  const sha1 = distribution ? current : find(profile.team);
  if (!sha1 || sha1 === '-' || teamOf(sha1, distribution) !== profile.team) {
    throw new Error(distribution
      ? `host/apple: the Developer ID profile "${profile.name}" is team ${profile.team}'s; sign with that team's Developer ID Application identity (EXACT_DEVELOPER_ID=<sha1>)`
      : `host/apple: the macOS profile "${profile.name}" is team ${profile.team}'s, and ${sha1} is not that team's Apple Development identity; name one with EXACT_IDENTITY=<sha1> (security find-identity -v -p codesigning)`);
  }
  return sha1;
}

/** How a macOS build of `app` signs, once its bake (`compat`) says what its grants need, resolved together:
 * `{ provisioning, widgets, sha1 }`. With Mac widgets (`widgets`, from `macWidgetSigning`, of the identity `current`'s
 * team) one profile serves both: it is chosen among the widgets' profiles that also allow the grants' needs, and the
 * identity stays `current`, so the widgets' App Group and the profile are one team's. Without widgets the profile's
 * team's identity signs (`macProfileIdentity`). An ad hoc development build (EXACT_IDENTITY=-) signs no profile's
 * entitlements. */
export function macSigning(app, compat, { current, widgets = null, release = false, distribution = false, log = console.log,
  find = macProfile, widgetProfiles = macWidgetProfiles, identityFor = macProfileIdentity, adHoc = !distribution && current === '-' } = {}) {
  let pair = null;
  const together = (a, required) => {
    pair = widgetProfiles({ app: a, team: widgets.team, distribution, log, also: (p) => required.every((name) => macAllows(p, a, name)) });
    return pair?.app ?? null;
  };
  const reason = adHoc ? 'this build is signed ad hoc (EXACT_IDENTITY=-), and no profile\'s entitlements hold under an ad hoc signature (LLP 1133.010 D7)' : null;
  const provisioning = macProvisioning(app, compat, { release, method: distribution ? 'developerid' : 'development', log, reason,
    beside: widgets ? `, with the Mac widgets' App Group and shared Keychain group for team ${widgets.team} (one profile signs both)` : '', find: widgets ? together : find });
  if (!provisioning) return { provisioning, widgets, sha1: current };
  if (widgets) return { provisioning, widgets: { ...widgets, profiles: pair }, sha1: current };
  return { provisioning, widgets, sha1: identityFor(provisioning.profile, current, { distribution }) };
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

/** The bake's associated domains for an entitlement. A passkey RP ID's `webcredentials:` in a development build is
 * `?mode=developer`, so the device fetches the association from the domain rather than Apple's CDN (LLP 1133.011
 * D6); a release build never asks for it. */
export function associatedDomains(auth, debuggable) {
  const passkeys = new Set((auth?.passkeys ?? []).map((rp) => `webcredentials:${rp}`));
  return (auth?.associatedDomains ?? []).map((d) => debuggable && passkeys.has(d) ? `${d}?mode=developer` : d);
}

/** Sign in with Apple's entitlement, which follows the signing, not the grant (LLP 1133.014 D7, D8): a simulator
 * takes it, and a device build takes it when its profile's entitlements (`allows`) include it. A development profile
 * and a distribution one carry it alike, when the App ID has the capability. Elsewhere it is left out, since an
 * entitlement the profile does not authorize stops the app installing. */
export function signInEntitlement(auth, { simulator = false, team = null, allows = [] } = {}) {
  const signable = (simulator && !team) || allows.includes('com.apple.developer.applesignin');
  return (auth?.signIn?.apple ?? []).length && signable ? { 'com.apple.developer.applesignin': ['Default'] } : {};
}

/** A grant's entitlement's value (`reach.entitlements`): `true`, or the string the build chooses (LLP 1133.028 D15
 * item 1). An environment follows the signing, not the compiler: `debuggable` is the signing profile's
 * `get-task-allow`, true on a simulator, so `development` there and under a development profile and `production`
 * under the others. App Attest's and push's (`aps-environment`, LLP 1133.004 §9 P2, which a Live Activity that takes
 * pushes signs too); Apple uses production for TestFlight and App Store builds whatever they say. */
export function entitlementValue(name, debuggable) {
  if (name === 'com.apple.developer.devicecheck.appattest-environment' || name === 'aps-environment') return signedEnvironment(debuggable);
  return true;
}

/** The grant entitlements (`reach.entitlements`) a bundle of `app` signs, and so the ones its profile must allow:
 * all of them for the app, none of push's for an App Clip, whose notifications are ephemeral (`NSAppClip`), or for an
 * extension, which reads no token (LLP 1133.004 §9 P2). The signature and the profile check both read this. */
export function grantEntitlements(app, names = []) {
  return app.clipParent || app.widgetAppId ? names.filter((name) => name !== 'aps-environment') : names;
}

/** @ref LLP 1133.034 D13 — the `payment.apple-pay` grants' merchant IDs, where they can be signed: on a simulator, or
 * with a profile whose entitlements (`allows`) include Apple Pay's. Elsewhere (an ad hoc Mac build) Apple Pay is
 * unavailable, and a profile without it would refuse the build. */
export function merchantEntitlement(reach, { simulator = false, team = null, allows = [] } = {}) {
  const merchants = reach?.payments?.merchantIds ?? [];
  const signable = (simulator && !team) || allows.includes('com.apple.developer.in-app-payments');
  return merchants.length && signable ? { 'com.apple.developer.in-app-payments': merchants } : {};
}
