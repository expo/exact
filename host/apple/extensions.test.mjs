import { expect, test } from 'bun:test';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { plistFile } from './assets.mjs';
import { EXTENSION_KINDS, extensionKinds, packageShareExtension, withoutExtensions } from './extensions.mjs';
import { entryProfiles, hasEntries, platformWidgetEntries, activityEntries } from './widgets.mjs';

test('--extensions keeps every kind by default, none, or a named list; refuses an unknown kind', () => {
  expect([...extensionKinds([])]).toEqual(EXTENSION_KINDS);
  expect([...extensionKinds(['--extensions', 'all'])]).toEqual(EXTENSION_KINDS);
  expect([...extensionKinds(['--extensions', 'none'])]).toEqual([]);
  expect([...extensionKinds(['--device', '--extensions', 'activities,widgets', 'apps/shelf'])]).toEqual(['activities', 'widgets']);
  expect(() => extensionKinds(['--extensions', 'widget'])).toThrow(/unknown kind widget/);
  expect(() => extensionKinds(['--extensions'])).toThrow(/takes all, none/);
  expect(() => extensionKinds(['--extensions', '--device'])).toThrow(/takes all, none/);
});

test('a left-out kind drops its manifest entries, so nothing gated on them builds', () => {
  const manifest = { widgets: [{ kind: 'next', families: ['systemSmall'] }], live_activities: [{ kind: 'trip' }], share_target: { action: '/receive' }, name: 'x' };
  const app = kinds => ({ manifest: withoutExtensions(manifest, new Set(kinds)) });
  expect(hasEntries(app(EXTENSION_KINDS))).toBe(true);
  expect(hasEntries(app([]))).toBe(false);
  expect(platformWidgetEntries(app(['activities'])).length).toBe(0);
  expect(activityEntries(app(['activities'])).length).toBe(1);
  expect(hasEntries(app(['activities']))).toBe(true); // the extension, with activities alone
  expect(activityEntries(app(['widgets'])).length).toBe(0);
  expect(app(['share']).manifest.share_target).toEqual(manifest.share_target);
  expect(app(['widgets']).manifest.share_target).toBeUndefined();
  expect(app([]).manifest.name).toBe('x');
  expect(manifest.widgets.length).toBe(1); // the app's own manifest is not changed
});

test('the share inbox requires an App Group profile for each bundle and one certificate that signs them all', () => {
  const id = 'com.example.league', group = `group.${id}`;
  const app = { id, manifest: { share_target: { action: '/receive' } } };
  const profile = (suffix, certificates, extra = {}) => ({
    appId: `TEAM.${id}${suffix}`, name: `League${suffix}`, prefix: 'TEAM', team: 'TEAM', method: 'development',
    expires: new Date('2099-01-01'), dev: true, devices: ['PHONE'], groups: [group], keychain: [], certificates, ...extra,
  });
  const choose = (installed, options = {}) => entryProfiles({ app, udid: 'PHONE', named: null, team: null, installed, ...options });
  const parent = profile('', ['A', 'B']), share = profile('.share', ['B']);
  expect(choose([parent, share])).toEqual({ app: parent, share });
  expect(() => choose([parent])).toThrow(`${id}.share`);
  expect(() => choose([parent, { ...share, groups: [] }])).toThrow(`none grants the App Group ${group}`);
  expect(() => choose([parent, { ...share, devices: ['OTHER'] }])).toThrow('does not include this phone');
  expect(() => choose([parent, { ...share, certificates: ['C'] }])).toThrow('common signing certificate');
  const otherTeam = { ...parent, team: 'OTHER', expires: new Date('2099-02-01') };
  expect(choose([otherTeam, parent, share]).app).toBe(parent);

  const widgets = { ...app, manifest: { ...app.manifest, live_activities: [{ kind: 'score' }] } };
  const keychain = [`TEAM.${id}`], owner = { ...parent, keychain };
  const widgetA = profile('.widgets', ['A'], { keychain }), widgetB = profile('.widgets', ['B'], { keychain });
  expect(() => choose([owner, widgetA, share], { app: widgets })).toThrow('common signing certificate');
  expect(choose([owner, widgetA, widgetB, share], { app: widgets })).toEqual({ app: owner, extension: widgetB, share });
  const store = p => ({ ...p, devices: [], dev: false, method: 'appstore' });
  expect(choose([store(share)], { udid: null, named: store(parent) }).share).toEqual(store(share));
});

// LLP 1133.024 D4, D6 (amended 2026-11-15): an extension compiles one presentation, so an app that declares none
// ships the compose sheet's extension unchanged.
test.skipIf(process.platform !== 'darwin')("the share extension compiles the presentation the app declares and names its controller", async () => {
  const { runApple } = await import('./build.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-share-presentation-'));
  const packaged = (apple, params = { text: 'text' }) => {
    const bundle = resolve(dir, JSON.stringify([apple ?? null, params]).replace(/\W+/g, '-'));
    let sources, defines;
    mkdirSync(bundle);
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile({ CFBundleVersion: '1', UIDeviceFamily: [1], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
    const extension = packageShareExtension({
      app: { id: 'dev.exact.presentation', displayName: 'Fixture', manifest: { share_target: { action: '/share', params, ...(apple ? { apple } : {}) } } },
      bundle, directory: resolve(bundle, 'build'), sdk: '/sdk', triple: 'fixture', group: 'group.dev.exact.presentation', entitlements: plistFile({}), sign() {},
      runApple: (command, args, options) => {
        if (command === 'plutil') return runApple(command, args, options);
        if (args.includes('swiftc')) {
          sources = args.filter(arg => arg.endsWith('.swift')).map(path => basename(path));
          defines = args.filter((arg, i) => args[i - 1] === '-D');
          copyFileSync('/usr/bin/true', args[args.indexOf('-o') + 1]);
        }
        return { status: 0 };
      },
    });
    const info = readFileSync(resolve(extension, 'Info.plist'), 'utf8');
    return { sources, defines, info, script: existsSync(resolve(extension, 'SharePage.js')), principal: info.match(/<key>NSExtensionPrincipalClass<\/key>\s*<string>([^<]*)<\/string>/)[1] };
  };
  try {
    const unsaid = packaged(), compose = packaged({ presentation: 'compose' }), confirmation = packaged({ presentation: 'confirmation' });
    expect(unsaid.sources).toEqual(['ShareInbox.swift', 'ShareViewController.swift']);
    expect(unsaid.principal).toBe('ExactShareViewController');
    expect(compose).toEqual(unsaid);
    expect(confirmation.sources).toEqual(['ShareInbox.swift', 'ShareConfirmation.swift']);
    expect(confirmation.principal).toBe('ExactShareConfirmationViewController');
    expect(confirmation.info.replace(confirmation.principal, unsaid.principal)).toBe(unsaid.info);
    // `postTitle` reaches the compose sheet as the word the app wrote.
    expect(packaged({ postTitle: 'Save it' }).info).toContain('<key>ExactSharePostTitle</key><string>Save it</string>');
    // Safari's page details are for an app that maps `url`: no other app declares the script, ships it, or
    // compiles its reader.
    expect([unsaid.defines, unsaid.script, unsaid.info.includes('NSExtensionJavaScriptPreprocessingFile')]).toEqual([[], false, false]);
    // An app's own dictionary rule is among them: it gains the web page.
    const ownRule = { NSExtensionActivationRule: { NSExtensionActivationSupportsWebURLWithMaxCount: 1 } };
    for (const page of [packaged(undefined, { text: 'text', url: 'url' }), packaged({ presentation: 'confirmation' }, { url: 'url' }), packaged(ownRule, { url: 'url' })]) {
      expect(page.defines).toEqual(['EXACT_SHARE_PAGE']);
      expect(page.script).toBe(true);
      expect(page.info).toMatch(/<key>NSExtensionJavaScriptPreprocessingFile<\/key>\s*<string>SharePage<\/string>/);
      expect(page.info).toMatch(/<key>NSExtensionActivationSupportsWebPageWithMaxCount<\/key>\s*<integer>1<\/integer>/);
    }
    // An app's own predicate is its rule as written, beside the script.
    const predicate = 'SUBQUERY(extensionItems, $item, SUBQUERY($item.attachments, $a, ANY $a.registeredTypeIdentifiers UTI-CONFORMS-TO "public.data").@count >= 1).@count >= 1';
    const owned = packaged({ NSExtensionActivationRule: predicate }, { url: 'url' });
    expect([owned.defines, owned.script]).toEqual([['EXACT_SHARE_PAGE'], true]);
    expect(owned.info).toContain(`<key>NSExtensionActivationRule</key><string>${predicate}</string>`);
    expect(owned.info).toMatch(/<key>NSExtensionJavaScriptPreprocessingFile<\/key>\s*<string>SharePage<\/string>/);
    expect(owned.info).not.toContain('NSExtensionActivationSupportsWebPageWithMaxCount');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// LLP 1133.024 D4 (amended 2026-11-15): what Exact's script answers Safari for a page. The extension's half of
// the same table is ShareExtensionTests' merge test.
test("the share extension's page script answers the page's title, its canonical web link or location as url, and a location that differs", () => {
  const source = readFileSync(new URL('./share/SharePage.js', import.meta.url), 'utf8');
  const selected = [];
  const answer = (location, canonical, title = 'Trip notes') => {
    const document = { title, location: { href: location },
      querySelector: selector => (selected.push(selector), canonical === undefined ? null : { href: canonical }) };
    let answered;
    new Function('document', `${source}; return ExtensionPreprocessingJS;`)(document).run({ completionFunction: value => { answered = value; } });
    return answered;
  };
  const limit = 4096, most = (1 << 20) - 4 * limit;
  const here = 'https://x.test/trip?utm_source=feed#top', canon = 'https://x.test/trip';
  const longest = `${here}${'a'.repeat(most - here.length)}`, tooLong = `${longest}a`;
  // Each title, and what the script answers for it: one over the bound is cut, on a whole character.
  const titles = [['Trip notes', 'Trip notes'], ['x'.repeat(2 << 20), 'x'.repeat(limit)], [`${'x'.repeat(limit - 1)}\u{1F600}`, 'x'.repeat(limit - 1)]];
  // [the page's location, its canonical link, the `url` answered, the `location` answered]
  const pages = [
    // No canonical web link, or one that is the location: one URL, sent once.
    [here, undefined, here], [here, here, here], [here, 'HTTPS://X.test:443/trip?utm_source=feed#top', here],
    [here, 'javascript:alert(1)', here], [here, 'data:text/html,x', here], [here, '', here],
    // A canonical link that differs is the `url`, and the location is sent beside it, a slash or a fragment apart included.
    [here, canon, canon, here], [`${canon}/`, canon, canon, `${canon}/`], [`${canon}#top`, canon, canon, `${canon}#top`],
    [`https://x.test/trip?${'a'.repeat(2 * limit)}`, canon, canon, `https://x.test/trip?${'a'.repeat(2 * limit)}`], [longest, canon, canon, longest],
    // A canonical link does not take an https page to http; an http page's own stands, and one to https.
    [here, 'http://x.test/trip', here], ['http://x.test/trip?a=1', 'http://x.test/trip', 'http://x.test/trip', 'http://x.test/trip?a=1'],
    ['http://x.test/trip?a=1', canon, canon, 'http://x.test/trip?a=1'],
    // A canonical link over the bound is not taken.
    [here, `https://x.test/${'a'.repeat(limit)}`, here],
    // No URL keeps a name or a password, and the location is compared without them.
    ['https://alice:s3cret@x.test/trip?a=1', undefined, 'https://x.test/trip?a=1'], ['https://alice@x.test/trip?a=1', 'https://bob:pw@x.test/trip', canon, 'https://x.test/trip?a=1'],
    ['https://alice:s3cret@x.test/trip', canon, canon],
    // A location too long to send beside a canonical link is left out; with no canonical link it is still the `url`.
    [tooLong, canon, canon], [tooLong, undefined, tooLong], [tooLong, 'http://x.test/trip', tooLong],
    // A page that is no web page has no location to give, and a `url` only from a canonical web link.
    ['file:///Users/a/trip.html', undefined], ['file:///Users/a/trip.html', canon, canon], ['file:///Users/a/trip.html', 'http://x.test/trip', 'http://x.test/trip'],
  ];
  for (const [location, canonical, url, sent] of pages) {
    for (const [title, cut] of titles) {
      const expected = { title: cut, ...(url ? { url } : {}), ...(sent ? { location: sent } : {}) };
      const got = answer(location, canonical, title);
      // Compared member by member: a megabyte of title is not printed when a row fails.
      expect([location.slice(0, 80), String(canonical).slice(0, 80), Object.keys(got).sort(), got.url === expected.url, got.location === expected.location, got.title === expected.title])
        .toEqual([location.slice(0, 80), String(canonical).slice(0, 80), Object.keys(expected).sort(), true, true, true]);
    }
  }
  expect(new Set(selected)).toEqual(new Set(['link[rel~="canonical" i][href]']));
});

// LLP 1156.006.000 D14: a module cache compiled at another path (a copied app folder) is removed and the step run once
// more; the path removed is the invocation's own argument, and another failure is left as it is.
test('a module cache compiled at another path is removed and the step tried again', async () => {
  const { runApple } = await import('./build.mjs'), { staleModuleCache } = await import('./extensions.mjs');
  const said = "error: PCH was compiled with module cache path '/old/app/target/cache', but the path is currently '/new/app/target/cache'";
  expect(staleModuleCache(['swiftc', '-module-cache-path', '/new/app/target/cache', 'a.swift'], said)).toBe('/new/app/target/cache');
  expect(staleModuleCache(['swiftc', 'a.swift'], said)).toBe(null);
  expect(staleModuleCache(['swiftc', '-module-cache-path', '/new/app/target/cache'], 'error: cannot find type')).toBe(null);
  const dir = mkdtempSync(join(tmpdir(), 'exact-stale-cache-')), cache = join(dir, 'cache');
  try {
    mkdirSync(cache); writeFileSync(join(cache, 'SwiftShims.pcm'), '');
    // Fails with clang's words while the cache holds a file, as a compile against a copied cache does.
    const script = `[ -e "$1/SwiftShims.pcm" ] && { echo "${said}" >&2; exit 1; }; exit 0`;
    expect(runApple('sh', ['-c', script, '-module-cache-path', cache], { stdio: 'pipe' }).status).toBe(0);
    expect(existsSync(cache)).toBe(false);
    expect(() => runApple('sh', ['-c', `echo "${said}" >&2; exit 1`, '-module-cache-path', cache], { stdio: 'pipe' })).toThrow('sh failed (1)');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
