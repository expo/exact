import { expect, test } from 'bun:test';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { chromium, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
import { runtimeSource } from '../web/tests/stand-ins.mjs';

const ROOT = resolve(import.meta.dirname, '../..');
const chromeTest = need('chrome', chromium().unavailable, { test });

// Time-to-interactive's outstanding work on the web (marks.js `outstanding`), with marks.js and held.js as they are,
// over a stand-in rt.js. The test counts tickets as `send` does: a stream leaves `inflight` at its first message and
// keeps its ticket (host/web/tests/js-runtime.test.mjs, "a stream answer settles per message").
test('an open stream hides none of the page loading that holds time-to-interactive, and a let-go request adds none', async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'exact-marks-'));
  writeFileSync(resolve(directory, 'marks.js'), runtimeSource('marks.js'));
  writeFileSync(resolve(directory, 'held.js'), runtimeSource('held.js'));
  writeFileSync(resolve(directory, 'rt.js'), `export const inflight = { n: 0 }, Resources = [], Mutations = [], clock = { timers: [] }, After = [], Hosts = {}, Routes = [];
export const routeAt = () => 0, busy = () => [];\nexport let Adopted = false;\n`);
  try {
    const rt = await import(pathToFileURL(resolve(directory, 'rt.js')).href);
    const { counted } = await import(pathToFileURL(resolve(directory, 'held.js')).href);
    const { outstanding } = await import(pathToFileURL(resolve(directory, 'marks.js')).href);
    const setWaiting = (ticket, waiting) => { ticket.waiting = waiting; counted(ticket, rt.inflight); };
    const stream = {}, save = {}, feed = { name: 'feed', placeholder: true, ticket: stream };
    rt.Resources.push(feed);
    rt.Mutations.push({ ticket: save });
    setWaiting(stream, true);
    expect(outstanding()).toEqual(['requests:feed']);
    // Deliver the stream's first message: its resource shows it and keeps the ticket for the messages to come
    setWaiting(stream, false);
    feed.placeholder = false;
    // Send a mutation's request: `inflight` counts it, and it holds no TTI
    setWaiting(save, true);
    expect(outstanding()).toEqual([]);
    // Start the page's own loading (a Markdown module, say)
    rt.inflight.n++;
    expect(outstanding()).toEqual(['loading:1']);
    // Add a request that its resource let go: it stays in `inflight` until its reply, which lands nowhere, and holds no TTI
    setWaiting({}, true);
    expect(outstanding()).toEqual(['loading:1']);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

// A served document painted before the runtime ran is this launch's presentation only when the boot adopts it
// (mount.js `Adopted`); a boot that renders fresh presents its own: here at a page rendered for another location
// (LLP 1048.000 D6), and where the plan does not fit the rendered page, so adoption is abandoned.
chromeTest('a launch presents the served document\'s first paint only when its boot adopts it', async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"><p>rendered</p></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    const local = resolve(ROOT, '.' + path), file = existsSync(local) ? local : resolve(ROOT, 'host/web', basename(path));
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
  const browser = await suiteBrowser();
  try {
    const launches = [];
    for (const [location, tag] of [['/', 'p'], ['/elsewhere', 'p'], ['/', 'div']]) {
      const page = await browser.newPage();
      await page.goto(server.url.href);
      launches.push(await page.evaluate(async ([location, tag]) => {
        // the served document's first contentful paint, before the runtime runs
        await new Promise(resolve => {
          const observer = new PerformanceObserver(list => {
            if (list.getEntries().some(e => e.name === 'first-contentful-paint')) {
              observer.disconnect();
              resolve();
            }
          });
          observer.observe({ type: 'paint', buffered: true });
        });
        const cp = document.createElement('script');
        cp.type = 'application/vnd.exact.checkpoint';
        cp.textContent = JSON.stringify({ location, answers: [] });
        document.head.append(cp);
        await import('/host/web-js/page-checkpoint.js'); // a rendered page's entry links its reader (build.mjs)
        const rt = await import('/host/web-js/rt.js'), marks = (await import('/host/web-js/marks.js')).install([]);
        rt.mount(root => rt.h(root, tag, 0, null, 'rendered'));
        marks.mounted();
        while (!exact.observe().marks.presentMethod) await new Promise(resolve => setTimeout(resolve, 50));
        const boot = rt.journal.map(l => / (adopted the document|adoption abandoned|document: rendered at \S+, not \S+;)/.exec(l)?.[1]).find(Boolean);
        return [boot, exact.observe().marks.presentMethod];
      }, [location, tag]));
      await page.close();
    }
    expect(launches.map(([boot]) => boot)).toEqual(['adopted the document', 'document: rendered at /elsewhere, not /;', 'adoption abandoned']);
    expect(launches[0][1]).toBe('served_fcp');
    for (const [, method] of launches.slice(1)) {
      expect(method).not.toContain('served');
    }
  } finally { await browser.close(); server.stop(true); }
}, 30000);
