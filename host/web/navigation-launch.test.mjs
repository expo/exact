import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { historyURL } from './navigation/routes.js';
import { launchLocation, launchVisits } from './navigation/environment.js';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { renderedPages } from '../web-js/rendered-pages.mjs';

// A drive's launch facts (scripts/agent-launch.mjs) are the carrier's, not the app's: none reaches the launch
// route, and every History entry keeps them, so a reload relaunches with them. `formatLocale` (#952) among them.
// One module serves both web hosts: the wasm glue and the JS target (rt.js) navigate through it.
const DRIVE = 'agent=1&seed=3&locale=en-US&formatLocale=en-KR&timeZone=UTC';
// The page's navigation entry names the URL the document launched at (Navigation Timing)
const launchedAt = url => ({ getEntriesByType: type => type === 'navigation' ? [{ name: url }] : [] });
// A platform at `path` in a document launched at `launch`
const at = (path, launch = path) => ({ location: new URL(path, 'http://127.0.0.1:9'), performance: launchedAt(new URL(launch, 'http://127.0.0.1:9').href) });

test('a drive\'s formatLocale stays out of the launch route', () => {
  expect(launchLocation(at(`/detail/3?${DRIVE}&q=a`))).toBe('/detail/3?q=a');
  // An ordinary visit's own query is the route's.
  expect(launchLocation(at('/?formatLocale=x'))).toBe('/?formatLocale=x');
  // Agent mode is the launch's: a location Back returns to keeps its own `agent=1` in an ordinary launch, and a drive's
  // location leaves the drive's parameters out
  expect(launchLocation(at('/one?agent=1&locale=fr&keep=yes', '/'))).toBe('/one?agent=1&locale=fr&keep=yes');
  expect(launchLocation(at(`/one?${DRIVE}&keep=yes`, `/?${DRIVE}`))).toBe('/one?keep=yes');
});

test('the launch keeps its fragment, after the query a drive leaves out (LLP 1153 D5)', () => {
  expect(launchLocation(at('/item/7#moves'))).toBe('/item/7#moves');
  expect(launchLocation(at(`/item/7?${DRIVE}&q=a#moves`))).toBe('/item/7?q=a#moves');
  // A history entry's URL carries the fragment after the drive's facts, as the browser writes it.
  const saved = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', { configurable: true, value: { origin: 'http://127.0.0.1:9', href: `http://127.0.0.1:9/?${DRIVE}` } });
  try {
    const url = new URL(historyURL('/item/7#moves'));
    expect([url.pathname, url.hash, url.searchParams.get('agent')]).toEqual(['/item/7', '#moves', '1']);
  } finally { if (saved) Object.defineProperty(globalThis, 'location', saved); else delete globalThis.location; }
});

test('a History entry keeps the userinfo of a page opened with one', () => {
  // `replaceState` refuses a URL whose username or password differs from the document's; it threw, and the boot ran again.
  const saved = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', { configurable: true, value: { origin: 'http://127.0.0.1:9', href: 'http://user:pass@127.0.0.1:9/' } });
  try {
    expect(historyURL('/item/7?q=a#moves')).toBe('http://user:pass@127.0.0.1:9/item/7?q=a#moves');
  } finally { if (saved) Object.defineProperty(globalThis, 'location', saved); else delete globalThis.location; }
});

test('a History entry is made under a location with no origin, as the render server gives', () => {
  // The JavaScript render server's placeholder location has an `href` and an empty `origin` (host/web-js/render.mjs).
  const saved = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', { configurable: true, value: { origin: '', href: 'http://render.invalid/' } });
  try {
    expect(historyURL('/item/7?q=a#moves')).toBe('http://render.invalid/item/7?q=a#moves');
  } finally { if (saved) Object.defineProperty(globalThis, 'location', saved); else delete globalThis.location; }
});

test('every History entry keeps a drive\'s formatLocale, so navigation and reload keep it', () => {
  const saved = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', { configurable: true, value: { origin: 'http://127.0.0.1:9', href: `http://127.0.0.1:9/?${DRIVE}` } });
  try {
    const next = new URL(historyURL('/next?page=2'));
    expect(next.pathname).toBe('/next');
    expect(Object.fromEntries(next.searchParams)).toEqual({ page: '2', agent: '1', seed: '3', locale: 'en-US', formatLocale: 'en-KR', timeZone: 'UTC' });
    // Reloaded there, the launch route is the app's alone again.
    expect(launchLocation(at(next.pathname + next.search))).toBe('/next?page=2');
  } finally { if (saved) Object.defineProperty(globalThis, 'location', saved); else delete globalThis.location; }
});

test('a History entry keeps a route whose first segment is empty, and a reload launches there', () => {
  // The router keeps `//b` as a path, never a network-path reference (route/tests/corpus.json).
  const saved = Object.getOwnPropertyDescriptor(globalThis, 'location');
  Object.defineProperty(globalThis, 'location', { configurable: true, value: { origin: 'http://127.0.0.1:9', href: 'http://127.0.0.1:9/' } });
  try {
    expect(historyURL('//b')).toBe('http://127.0.0.1:9//b');
    expect(historyURL('///b')).toBe('http://127.0.0.1:9///b');
    const entry = new URL(historyURL('//item/7?q=a#moves'));
    expect(entry.href).toBe('http://127.0.0.1:9//item/7?q=a#moves');
    expect(launchLocation({ location: entry })).toBe('//item/7?q=a#moves');
  } finally { if (saved) Object.defineProperty(globalThis, 'location', saved); else delete globalThis.location; }
});

test('a reload\'s launch has the visits Back returns to without loading a page, each as this launch reads it', () => {
  const entry = (path, sameDocument = true) => ({ url: `http://127.0.0.1:9${path}`, sameDocument });
  // After a reload, the current entry is the URL the document launched at
  const navigation = (entries, index) => ({ navigation: { entries: () => entries, currentEntry: { index } }, performance: launchedAt(entries[index]?.url) });
  // An entry of another document, which Back loads, and those before it are no visit of this launch.
  expect(launchVisits(navigation([entry('/elsewhere', false), entry(`/?${DRIVE}`), entry(`/settings?${DRIVE}#top`), entry(`/preview?${DRIVE}`)], 3))).toEqual(['/', '/settings#top']);
  // Agent mode is the launch's: in an ordinary launch a visit's own `agent` is the app's query, kept as it is
  expect(launchVisits(navigation([entry('/'), entry('/next?agent=1&locale=fr'), entry('/end')], 2))).toEqual(['/', '/next?agent=1&locale=fr']);
  expect(launchVisits(navigation([entry('/')], 0))).toEqual([]);
  expect(launchVisits({})).toEqual([]);
});

// #527: after a reload on a deep route, the browser's Back pops to the visit beneath, as before the reload, through the
// root's `navigate` (`go`), and does not push it over the route Back leaves; the app bar's Back steps back to that entry.
test('after a reload, the browser\'s Back and the app bar\'s pop to the visit beneath', () => renderedPages(`routes nav
  home "/"
    settings "/settings"
    preview "/preview"

component Visits
  action open(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` position="absolute" inset=0 testId=\`route-\${e.name}\`
          header
            h1
              text e.name
          if e.name == "home"
            button press=open("/settings") testId="open-settings"
              text "Settings"
          if e.name == "settings"
            button press=open("/preview") testId="open-preview"
              text "Preview"
`, async (_, __, dist) => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    const file = Bun.file(resolve(dist, new URL(req.url).pathname.slice(1)));
    return await file.exists() && !(await file.stat()).isDirectory() ? new Response(file) : new Response(Bun.file(resolve(dist, 'index.html')));
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    const stack = async (path) => {
      await page.waitForFunction((p) => location.pathname === p && !!globalThis.exact?.ready && !document.querySelector('[data-exiting]'), path);
      return page.evaluate(() => [...document.querySelectorAll('[data-testid^="route-"]')].map((r) => r.dataset.testid.slice(6)));
    };
    await page.goto(server.url.href);
    await stack('/');
    await page.click('[data-testid="open-settings"]');
    await stack('/settings');
    await page.click('[data-testid="open-preview"]');
    expect(await stack('/preview')).toEqual(['home', 'settings', 'preview']);
    await page.reload();
    expect(await stack('/preview')).toEqual(['home', 'settings', 'preview']);
    // The app bar's Back steps back to the entry beneath, as the browser's does, and pushes none.
    const entries = () => page.evaluate(() => [navigation.currentEntry.index, navigation.entries().length]);
    const [at, count] = await entries();
    await page.evaluate(() => document.querySelector('[data-testid="route-preview"] > header').shadowRoot.querySelector('button').click());
    expect(await stack('/settings')).toEqual(['home', 'settings']);
    expect(await entries()).toEqual([at - 1, count]);
    await page.evaluate(() => history.back());
    expect(await stack('/')).toEqual(['home']);
    await page.evaluate(() => history.forward());
    expect(await stack('/settings')).toEqual(['home', 'settings']);
    await page.evaluate(() => history.forward());
    expect(await stack('/preview')).toEqual(['home', 'settings', 'preview']);
    await page.reload();
    expect(await stack('/preview')).toEqual(['home', 'settings', 'preview']);
    await page.evaluate(() => history.back());
    expect(await stack('/settings')).toEqual(['home', 'settings']);
    await page.evaluate(() => history.back());
    expect(await stack('/')).toEqual(['home']);
    await page.evaluate(() => history.forward());
    expect(await stack('/settings')).toEqual(['home', 'settings']);
  } finally { await browser.close(); server.stop(true); }
}), 180000);
