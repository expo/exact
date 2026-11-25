// Verified https links (LLP 1038 D8): the declaration's refusals, the claims each host derives and the files each
// origin serves.
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidLinkFilters, androidLinkNotes, appleLinkDomains, claimedOrigin, defaultScope, linkClaims, processedId } from './links.mjs';
import { appleAssociation, androidStatements, elsewhereFiles, writeLinkFiles } from '../host/web/association.mjs';

const CERT = Array(32).fill('AB').join(':');
// LLP 1038 D8's example.
const notes = { name: 'Notes', app: { id: 'com.example.notes', name: 'Notes', origin: 'https://notes.example' }, start_url: '/notes/', scope: '/notes/',
  handle_links: 'preferred', scope_extensions: [{ type: 'origin', origin: 'https://share.example' }], host: { ios: { team: 'TEAM' }, android: { certificates: [CERT] } } };
const appOf = (manifest) => ({ manifest, id: manifest.app.id, origin: manifest.app.origin ?? null });

test('https auth callbacks publish credentials and verified returns without claiming navigation', () => {
  const app = appOf({ ...notes, handle_links: 'auto', scope_extensions: [] });
  const callbacks = ['https://notes.example/cb', 'https://auth.example/cb', 'app.test:/cb'];
  const options = { callbacks };
  assert.deepEqual(appleAssociation(app, options), { webcredentials: { apps: ['TEAM.com.example.notes'] } });
  assert.equal(androidStatements(app, options)[0].relation[0], 'delegate_permission/common.handle_all_urls');
  const elsewhere = elsewhereFiles(app, options);
  assert.deepEqual(elsewhere.map((entry) => entry.origin), ['https://auth.example']);
  assert.deepEqual(elsewhere[0].files['apple-app-site-association'], { webcredentials: { apps: ['TEAM.com.example.notes'] } });
  assert.equal(elsewhere[0].files['assetlinks.json'][0].target.package_name, 'com.example.notes');
  const merged = elsewhereFiles(appOf(notes), { callbacks: ['https://share.example/cb'] })[0].files;
  assert.deepEqual(Object.keys(merged['apple-app-site-association']), ['applinks', 'webcredentials']);
  assert.deepEqual(merged['apple-app-site-association'].webcredentials.apps, ['TEAM.com.example.notes']);
  assert.equal(merged['assetlinks.json'].length, 1);
  assert.deepEqual(appleAssociation({ ...app, origin: `${app.origin}/` }, options), appleAssociation(app, options));
  const root = mkdtempSync(resolve(tmpdir(), 'exact-auth-links-'));
  try {
    writeLinkFiles(root, app, options);
    assert.deepEqual(JSON.parse(readFileSync(resolve(root, '.well-known/apple-app-site-association'))), appleAssociation(app, options));
    assert.deepEqual(JSON.parse(readFileSync(resolve(root, '.well-known/assetlinks.json'))), androidStatements(app, options));
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test('escaped web popup callbacks derive no native domain association', () => {
  const app = appOf({ ...notes, handle_links: 'auto', scope_extensions: [] });
  for (const path of ['/.exact/auth/callback', '/%2Eexact/auth/callback', '/site/%2eexact/auth%2Fcallback']) {
    const options = { callbacks: [`https://notes.example${path}`, `https://auth.example${path}`] };
    assert.equal(appleAssociation(app, options), null, path);
    assert.deepEqual(androidStatements(app, options), [], path);
    assert.deepEqual(elsewhereFiles(app, options), [], path);
  }
});

test('callback-only association files name missing signing identities', () => {
  const root = mkdtempSync(resolve(tmpdir(), 'exact-auth-links-missing-'));
  try {
    for (const host of [{}, { ios: notes.host.ios }, { android: notes.host.android }]) {
      const app = appOf({ ...notes, handle_links: 'auto', scope_extensions: [], host });
      const lines = writeLinkFiles(root, app, { callbacks: ['https://notes.example/auth/return'] }).join('\n');
      assert.match(lines, /notes.example/);
      assert.equal(lines.includes('no host.ios.team'), !host.ios);
      assert.equal(lines.includes('no host.android.certificates'), !host.android);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('Android 11 verification notes count native callback hosts beside navigation hosts', () => {
  const manifest = { ...notes, scope_extensions: [], host: { android: { minimumOS: '11' } } };
  const filters = androidLinkFilters(manifest);
  const callbacks = ['https://auth.example/return', 'https://auth.example/other', 'https://notes.example/cb'];
  assert.deepEqual(androidLinkNotes(manifest, filters, { callbacks }), [
    'App Links: below Android 12 one unverified host of notes.example, auth.example leaves all of them unverified']);
  assert.deepEqual(androidLinkNotes(manifest, filters, { callbacks: [
    'https://notes.example/cb', 'https://notes.example/other', 'https://popup.example/.exact/auth/callback',
    'https://popup.example/%2eexact/auth/callback', 'notes:/cb', 'http://127.0.0.1/return',
  ] }), []);
  assert.deepEqual(androidLinkNotes({ ...manifest, host: { android: { minimumOS: '12' } } }, filters, { callbacks }), []);
});

test('an external origin gets one association file for callbacks, passkeys and navigation', () => {
  const passkeys = ['share.example', 'passkeys.example', 'localhost', 'share.example'];
  const files = elsewhereFiles(appOf(notes), { callbacks: ['https://share.example/return'], passkeys });
  assert.deepEqual(files.map(entry => entry.origin), ['https://share.example', 'https://passkeys.example']);
  const shared = files[0].files;
  assert.deepEqual(Object.keys(shared['apple-app-site-association']), ['applinks', 'webcredentials']);
  assert.deepEqual(shared['apple-app-site-association'].webcredentials.apps, ['TEAM.com.example.notes']);
  assert.deepEqual(shared['assetlinks.json'].map(statement => statement.relation[0]), [
    'delegate_permission/common.handle_all_urls', 'delegate_permission/common.get_login_creds']);
  assert.deepEqual(files[1].files['assetlinks.json'].map(statement => statement.relation[0]), ['delegate_permission/common.get_login_creds']);
});

test('the declaration is refused where it is wrong, every problem named, and the removed boolean by name', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-links-'));
  const read = (manifest) => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest)); return readManifest(dir, 'notes'); };
  try {
    assert.equal(read(notes).handle_links, 'preferred');
    for (const [change, refusal] of [
      [{ handle_links: 'always' }, /handle_links: "always" is not one of "auto", "preferred", "not-preferred"/],
      [{ app: { id: 'com.example.notes', name: 'Notes' } }, /handle_links: "preferred" claims app.origin, which the manifest does not declare/],
      [{ app: { ...notes.app, origin: 'https://notes.example:8443' } }, /app.origin: https:\/\/notes.example:8443 names port 8443/],
      [{ scope: 'notes/' }, /scope: "notes\/" is not an absolute path/],
      [{ scope: '/notes/*' }, /scope: "\/notes\/\*" contains \*/],
      [{ scope: '/a/../notes/' }, /dot segment/],
      [{ scope: '/other/' }, /start_url: \/notes\/ is outside scope \/other\//],
      [{ scope_extensions: [{ type: 'url', origin: 'https://share.example' }] }, /scope_extensions\[0\].type: "url" is not "origin"/],
      [{ scope_extensions: [{ type: 'origin', origin: 'http://share.example' }] }, /is not https/],
      [{ scope_extensions: [{ type: 'origin', origin: 'https://share.example/x' }] }, /carries a path/],
      [{ scope_extensions: [{ type: 'origin', origin: 'https://*.share.example' }] }, /wildcard origin/],
      [{ scope_extensions: [{ type: 'origin', origin: 'https://NOTES.example:443' }] }, /repeats app.origin/],
      [{ handle_links: 'not-preferred', host: { ios: { associatedDomains: ['applinks:other.example'] } } }, /"not-preferred" beside host.ios.associatedDomains applinks:other.example/],
      [{ host: { ios: { associatedDomains: ['applinks:*.example'] } } }, /applinks:\*.example covers notes.example, which handle_links claims/],
      [{ host: { ios: { associatedDomains: true } } }, /host.ios.associatedDomains: true was removed: declare "handle_links": "preferred"/],
      [{ host: { ios: { associatedDomains: false } } }, /host.ios.associatedDomains: false was removed: delete it/],
    ]) assert.throws(() => read({ ...notes, ...change }), refusal, JSON.stringify(change));
    // Raw entries for what has no web name stay, a webcredentials: and a host the claims do not name among them.
    assert.ok(read({ ...notes, host: { ios: { associatedDomains: ['webcredentials:notes.example', 'applinks:other.example'] } } }));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('origins are normalized, the scope defaults to start_url\'s directory and the processed id is a URL', () => {
  assert.deepEqual(claimedOrigin('https://BÜCHER.example:443'), { origin: 'https://xn--bcher-kva.example', host: 'xn--bcher-kva.example' });
  assert.deepEqual(['/', '/notes', '/notes/', '/notes/index.html', '/notes/?a#b'].map(defaultScope), ['/', '/', '/notes/', '/notes/', '/notes/']);
  assert.equal(processedId(notes), 'https://notes.example/notes/');
  assert.equal(processedId({ ...notes, id: 'com.exact.caltrain' }), 'https://notes.example/com.exact.caltrain');
  assert.equal(processedId({ ...notes, app: { id: 'x' } }), null);
});

test('preferred claims app.origin within its scope and each extension whole; nothing else claims natively', async () => {
  assert.deepEqual(appleLinkDomains(notes), ['applinks:notes.example', 'applinks:share.example']);
  for (const handle_links of ['auto', 'not-preferred', undefined]) assert.deepEqual(linkClaims({ ...notes, handle_links }), []);
  // Exact's paths and an https callback on a claimed origin are left to the browser; Android names those in its scope.
  const callbacks = ['https://notes.example/notes/cb', 'https://share.example/done', 'https://notes.example/.exact/auth/callback', 'notes:/cb'];
  assert.deepEqual(linkClaims(notes, callbacks).map((c) => c.exclude), [['/.exact/', '/notes/cb'], ['/done']]);
  assert.deepEqual(androidLinkFilters(notes, callbacks), [
    { host: 'notes.example', pathPrefix: '/notes/', exclude: [], allow: '/notes/', claimed: ['/notes/cb'] },
    { host: 'share.example', pathPrefix: null, exclude: ['/done'], allow: '/', claimed: [] }]);
  // A narrower scope's own path would match first, so its exclusions move the scope into the groups from 15 on.
  const from15 = { ...notes, host: { android: { minimumOS: '15' } } };
  assert.deepEqual(androidLinkFilters(from15, callbacks)[0], { host: 'notes.example', pathPrefix: null, exclude: ['/notes/cb'], allow: '/notes/', claimed: [] });
  assert.deepEqual(androidLinkFilters({ ...notes, scope: '/', start_url: '/' })[0], { host: 'notes.example', pathPrefix: null, exclude: ['/.exact/'], allow: '/', claimed: [] });
  assert.match(androidLinkNotes(notes, androidLinkFilters(notes, callbacks), { signsIn: true }).at(-1), /notes.example\/notes\/cb opens the app rather than the browser on every Android release/);
  // The notes name the limits of the releases an APK supports.
  const filters = androidLinkFilters({ ...notes, scope: '/', start_url: '/' });
  assert.deepEqual(androidLinkNotes({ ...notes, host: { android: { minimumOS: '15' } } }, filters, { signsIn: true }), []);
  assert.deepEqual(androidLinkNotes(notes, filters), [], 'an app that signs in nowhere');
  const old = androidLinkNotes({ ...notes, host: { android: { minimumOS: '11' } } }, filters, { signsIn: true });
  assert.equal(old.length, 2);
  assert.match(old[0], /below Android 12 one unverified host of notes.example, share.example/);
  assert.match(androidLinkNotes({ ...notes, host: { android: { minimumOS: '14' } } }, filters, { signsIn: true })[0], /below Android 15 notes.example\/.exact\/ opens the app rather than the browser/);
  // The binary's metadata: Apple's signed hosts, Android's hosts and paths.
  const { nativeManifestMetadata, webAppManifest } = await import('./app.mjs');
  assert.deepEqual(nativeManifestMetadata(notes, 'ios').links, ['applinks:notes.example', 'applinks:share.example']);
  assert.deepEqual(nativeManifestMetadata(notes, 'android').links.map((f) => [f.host, f.pathPrefix]), [['notes.example', '/notes/'], ['share.example', null]]);
  assert.equal(nativeManifestMetadata(notes, 'web').links, undefined);
  // The web manifest copies the three as written.
  const web = webAppManifest({ ...notes, handle_links: 'auto' }, 'Notes');
  assert.deepEqual([web.handle_links, web.scope, web.scope_extensions], ['auto', '/notes/', notes.scope_extensions]);
});

test('the deployed origin\'s files are written merged and every other origin\'s are printed (LLP 1038 D8 example)', () => {
  const app = appOf(notes);
  assert.deepEqual(appleAssociation(app), { applinks: { details: [{ appIDs: ['TEAM.com.example.notes'], components: [{ '/': '/.exact/*', exclude: true }, { '/': '/notes/*' }] }] } });
  const links = { relation: ['delegate_permission/common.handle_all_urls'], target: { namespace: 'android_app', package_name: 'com.example.notes', sha256_cert_fingerprints: [CERT] } };
  assert.deepEqual(androidStatements(app, { passkeys: true }), [links, { ...links, relation: ['delegate_permission/common.get_login_creds'] }]);
  assert.deepEqual(elsewhereFiles(app), [{ origin: 'https://share.example', files: {
    'apple-app-site-association': { applinks: { details: [{ appIDs: ['TEAM.com.example.notes'], components: [{ '/': '/*' }] }] } },
    'assetlinks.json': [links], 'web-app-origin-association': { 'https://notes.example/notes/': {} } } }]);
  // The web file is printed whatever handle_links says; raw applinks hosts print their Apple file.
  assert.deepEqual(elsewhereFiles(appOf({ ...notes, handle_links: undefined, host: { ios: { team: 'TEAM', associatedDomains: ['applinks:*.raw.example'] } } }))
    .map(({ origin, files }) => [origin, Object.keys(files)]), [['https://share.example', ['web-app-origin-association']], ['https://raw.example', ['apple-app-site-association']]]);
  const root = mkdtempSync(resolve(tmpdir(), 'exact-link-files-'));
  try {
    const lines = writeLinkFiles(root, app, { passkeys: ['notes.example'] });
    assert.deepEqual(Object.keys(JSON.parse(readFileSync(resolve(root, '.well-known/apple-app-site-association'), 'utf8'))), ['applinks', 'webcredentials']);
    assert.equal(JSON.parse(readFileSync(resolve(root, '.well-known/assetlinks.json'), 'utf8')).length, 2);
    assert.equal(lines[0], 'links: notes.example/notes/, share.example open the app');
    assert.deepEqual(lines.slice(2).map((l) => l.split(' ').slice(0, 5).join(' ')), ['links: https://share.example must serve /.well-known/apple-app-site-association',
      'links: https://share.example must serve /.well-known/assetlinks.json', 'links: https://share.example must serve /.well-known/web-app-origin-association']);
    // Without a team or certificates the build names what it could not write.
    rmSync(resolve(root, '.well-known'), { recursive: true });
    const bare = writeLinkFiles(root, appOf({ ...notes, host: {} }));
    assert.match(bare[0], /no host.ios.team \(no apple-app-site-association: iOS and macOS verify none\), no host.android.certificates/);
    assert.equal(existsSync(resolve(root, '.well-known')), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the files name the base app, whatever variant the build is for (LLP 1030.008 D8)', () => {
  const variant = { ...notes, app: { ...notes.app, id: 'com.example.notes.dev' }, variant: { name: 'dev', base: 'com.example.notes' },
    host: { ...notes.host, android: { ...notes.host.android, id: 'com.example.notes_android.dev' }, ios: { team: 'TEAM', appClip: {} } } };
  const app = appOf(variant);
  assert.deepEqual(appleAssociation(app).applinks.details[0].appIDs, ['TEAM.com.example.notes']);
  assert.deepEqual(appleAssociation(app).appclips.apps, ['TEAM.com.example.notes.Clip']);
  assert.equal(androidStatements(app)[0].target.package_name, 'com.example.notes_android');
});

test('the deployed origin\'s Apple file: applinks where a claim covers its host, the App Clip beside them', () => {
  const app = { id: 'com.exact.test', displayName: 'Test', origin: 'https://exact.test', manifest: { handle_links: 'preferred', app: { id: 'com.exact.test', origin: 'https://exact.test' }, host: { ios: { appClip: {}, team: 'TEAM' } } } };
  assert.deepEqual(appleAssociation(app), {
    applinks: { details: [{ appIDs: ['TEAM.com.exact.test'], components: [{ '/': '/.exact/*', exclude: true }, { '/': '/*' }] }] },
    appclips: { apps: ['TEAM.com.exact.test.Clip'] },
  });
  assert.equal(appleAssociation({ ...app, manifest: { ...app.manifest, host: { ios: { appClip: {} } } } }), null, 'no team, no file');
  const listed = (associatedDomains) => ({ ...app, manifest: { host: { ios: { associatedDomains, appClip: {}, team: 'TEAM' } } } });
  assert.deepEqual(Object.keys(appleAssociation({ ...listed([]) }, { webcredentials: true })), ['webcredentials']);
  assert.deepEqual(appleAssociation(listed(['applinks:exact.test'])).applinks.details[0].components, [{ '/': '/.exact/*', exclude: true }, { '/': '/*' }]);
  assert.equal(appleAssociation(listed(['applinks:other.example'])), null);
  assert.equal(appleAssociation(listed([])), null);
  assert.deepEqual(Object.keys(appleAssociation(listed(['applinks:exact.test?mode=developer']))), ['applinks', 'appclips']);
  assert.deepEqual([['applinks:*.exact.test'], ['applinks:*.test'], ['applinks:*.xexact.test']].map((l) => !!appleAssociation(listed(l))), [true, true, false]);
  assert.equal(appleAssociation({ ...app, origin: undefined }), null, 'no origin, no host to claim');
});

test('Android: one autoVerify https filter per claimed host, its scope and, from 15, its exclusions', async () => {
  const { withAppLinks, withProtocolHandlers, withAuthCallbacks } = await import('./android-manifest.mjs');
  assert.equal(withAppLinks('unchanged', []), 'unchanged');
  const xml = withAuthCallbacks(withAppLinks(withProtocolHandlers('', { protocol_handlers: [{ protocol: 'notes', url: '/open?u=%s' }] }),
    androidLinkFilters({ ...notes, scope: '/', start_url: '/' }, ['https://notes.example/cb'])), ['notes:/cb']);
  assert.equal((xml.match(/<activity /g) ?? []).length, 1);
  assert.equal((xml.match(/android:autoVerify="true"/g) ?? []).length, 2);
  assert.match(xml, /<data android:scheme="https" \/>\n\s*<data android:host="notes.example" \/>\n\s*<uri-relative-filter-group android:allow="false">\n\s*<data android:pathPrefix="\/.exact\/" \/>\n\s*<\/uri-relative-filter-group>\n\s*<uri-relative-filter-group android:allow="false">\n\s*<data android:path="\/cb" \/>\n\s*<\/uri-relative-filter-group>\n\s*<uri-relative-filter-group>\n\s*<data android:pathPrefix="\/" \/>/);
  assert.match(xml, /<data android:host="share.example" \/>\n\s*<\/intent-filter>/);
  const scoped = withAppLinks('', androidLinkFilters(notes));
  assert.match(scoped, /<data android:host="notes.example" \/>\n\s*<data android:pathPrefix="\/notes\/" \/>\n\s*<\/intent-filter>/);
});

test('a Mac profile signs the links when it can, and a build without one says links open in the browser', async () => {
  const { macProfileNeeds, macProfileEntitlements } = await import('../host/apple/presence.mjs');
  const app = appOf(notes);
  const needs = macProfileNeeds({ inputs: { grantCeiling: '' }, reach: {} }, app);
  assert.deepEqual(needs, [{ entitlement: 'com.apple.developer.associated-domains', lines: 'handle_links "preferred"', mandatory: false,
    effect: 'links to notes.example, share.example open in the browser' }]);
  const signed = macProfileEntitlements(app, { prefix: 'TEAM', team: 'TEAM', dev: false }, needs, { reach: { auth: { associatedDomains: ['webcredentials:notes.example'] } } });
  assert.deepEqual(signed['com.apple.developer.associated-domains'], ['webcredentials:notes.example', 'applinks:notes.example', 'applinks:share.example']);
});


test('root bases keep native claims and other link preferences permit a mounted site', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-link-base-'));
  try {
    for (const base of [undefined, '/']) {
      const manifest = { ...notes, host: { ...notes.host, web: { base } } };
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
      assert.deepEqual(linkClaims(readManifest(dir, 'notes')), linkClaims(notes));
    }
    for (const handle_links of [undefined, 'auto', 'not-preferred']) {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...notes, handle_links, host: { web: { base: '/app/' } } }));
      assert.deepEqual(linkClaims(readManifest(dir, 'notes')), []);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Android excludes the Exact directory and its descendants', async () => {
  const { withAppLinks } = await import('./android-manifest.mjs');
  const manifest = { ...notes, start_url: '/', scope: '/', host: {
    ...notes.host, web: { base: '/' }, android: { ...notes.host.android, minimumOS: '15' },
  } };
  const xml = withAppLinks('', androidLinkFilters(manifest));
  assert.match(xml, /android:allow="false">\s*<data android:pathPrefix="\/\.exact\/"/);
  assert.doesNotMatch(xml, /android:path="\/\.exact\/"/);
  assert.match(xml, /<uri-relative-filter-group>\s*<data android:pathPrefix="\/"/);
});

test('mounted preferred claims publish the scope and exclude the mounted Exact tree', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-mounted-links-'));
  try {
    const manifest = { ...notes, host: { ...notes.host, web: { base: '/app/' }, android: { minimumOS: '15' } } };
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
    const app = appOf(readManifest(dir, 'notes'));
    assert.deepEqual(appleAssociation(app).applinks.details[0].components, [{ '/': '/app/.exact/*', exclude: true }, { '/': '/app/notes/*' }]);
    assert.equal(androidLinkFilters(app.manifest)[0].pathPrefix, '/app/notes/');
    const { withAppLinks } = await import('./android-manifest.mjs');
    const whole = { ...app.manifest, start_url: '/', scope: '/' };
    assert.match(withAppLinks('', androidLinkFilters(whole)), /android:pathPrefix="\/app\/\.exact\/"/);
    assert.equal(linkClaims(app.manifest)[1].scope, '/');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('native metadata and Apple plists bake the incoming web mount', async () => {
  const { nativeManifestMetadata } = await import('./app.mjs');
  const { infoPlist, macInfoPlist } = await import('../host/apple/build.mjs');
  const manifest = { ...notes, host: { ...notes.host, web: { base: '/app/' } } };
  const mount = { origin: notes.app.origin, base: '/app/' };
  for (const platform of ['ios', 'macos', 'android', 'linux', 'windows']) {
    assert.deepEqual(nativeManifestMetadata(manifest, platform).webMount, mount);
    assert.equal(nativeManifestMetadata(notes, platform).webMount, undefined);
  }
  for (const plist of [infoPlist({ ...appOf(manifest), displayName: 'Notes' }), macInfoPlist({ ...appOf(manifest), displayName: 'Notes' })]) {
    assert.match(plist, /<key>ExactWebMount<\/key>/);
    assert.match(plist, /<key>base<\/key>\s*<string>\/app\/<\/string>/);
  }
});

test('JavaScript derives callback associations and preserves a bake\'s passkeys', () => {
  const root = resolve(import.meta.dir, '..'), dir = mkdtempSync(resolve(root, 'target/web-js-associations-'));
  const cert = Array(32).fill('AB').join(':');
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Associations', rust: false,
      app: { id: 'test.associations', name: 'Associations', origin: 'https://app.example' },
      host: { ios: { team: 'TEAM' }, android: { certificates: [cert] } } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  resource label = label() as shape string\n  view\n    text label\n');
    writeFileSync(resolve(dir, 'app.ts'), "export const grants = 'auth.passkey app.example\\nauth.passkey auth.example\\nauth.session https://as.test\\nauth.callback https://app.example/auth/return\\nauth.callback https://auth.example/return';\nexport const answer = () => 'Sign in';\n");
    const env = { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST: resolve(dir, 'bake') };
    const standalone = resolve(dir, 'standalone');
    const unbaked = spawnSync(process.execPath, ['host/web-js/build.mjs', 'associations', '--out', standalone, '--render', 'none'], {
      cwd: root, env, encoding: 'utf8',
    });
    expect(unbaked.status, unbaked.stdout + unbaked.stderr).toBe(0);
    expect(JSON.parse(readFileSync(resolve(standalone, '.well-known/apple-app-site-association'))).webcredentials.apps).toEqual(['TEAM.test.associations']);
    expect(JSON.parse(readFileSync(resolve(standalone, '.well-known/assetlinks.json'))).map(s => s.relation[0])).toEqual([
      'delegate_permission/common.handle_all_urls']);
    expect(unbaked.stdout + unbaked.stderr).not.toContain('delegate_permission/common.get_login_creds');
    const baked = spawnSync(process.execPath, ['host/web/build.mjs', 'associations', '--bake'], { cwd: root, env, encoding: 'utf8' });
    expect(baked.status, baked.stdout + baked.stderr).toBe(0);
    const output = baked.stdout + baked.stderr;
    const external = output.split('\n').filter(line => line.startsWith('links: https://auth.example must serve /.well-known/assetlinks.json '));
    expect(external).toHaveLength(1);
    expect(JSON.parse(external[0].slice(external[0].indexOf(' [') + 1)).map(statement => statement.relation[0])).toEqual([
      'delegate_permission/common.handle_all_urls', 'delegate_permission/common.get_login_creds']);
    expect(output.match(/\/\.well-known\/assetlinks\.json /g)).toHaveLength(1);
    const bake = env.EXACT_WEB_DIST, out = resolve(dir, 'web');
    const names = ['apple-app-site-association', 'assetlinks.json'];
    const files = names.map(name => readFileSync(resolve(bake, '.well-known', name), 'utf8'));
    expect(JSON.parse(files[0]).webcredentials.apps).toEqual(['TEAM.test.associations']);
    expect(JSON.parse(files[1]).map(s => s.relation[0])).toEqual([
      'delegate_permission/common.handle_all_urls', 'delegate_permission/common.get_login_creds']);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'associations', '--production', '--plan', resolve(bake, 'app.plan'), '--out', out, '--render', 'none'], {
      cwd: root, env, encoding: 'utf8',
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(names.map(name => readFileSync(resolve(out, '.well-known', name), 'utf8'))).toEqual(files);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);
