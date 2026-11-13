// The page module under a static host's content security policy: the JS
// target's native.js and the glue it imports, served as an app's build serves
// them, load the app's module artifact and answer `native.later` in headless
// Chrome. A static host's policy is the site's, not the render server's
// (host.web.csp), so it may admit no inline script at all.
import { afterAll, beforeAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { cdpBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { readyPage } from './page.mjs';

const { unavailable } = chromium(), check = need('chrome', unavailable);
const file = path => readFileSync(new URL(path, import.meta.url), 'utf8');
// `default-src 'self'` alone, and a gateway's (Filebase sends this one).
const POLICIES = ["default-src 'self'", "default-src 'self'; img-src * data: blob: 'unsafe-inline'; style-src * 'unsafe-inline';"];

const PAGE = '<!doctype html><meta charset="utf-8"><title>CSP</title><script type="module" src="./boot.js"></script>';
// The page's own script, external as the build's app.js is: the violations
// the policy reports, then the module's answer to one request.
const BOOT = `
window.violations = [];
document.addEventListener('securitypolicyviolation', e => violations.push(e.violatedDirective + ' ' + (e.blockedURI || 'inline')));
globalThis.exact = {};
const { pageModule } = await import('./native.js');
const announced = [];
try {
  const page = await pageModule({ changed: topic => announced.push(topic), agent: false, now: () => 0 });
  window.answer = await page.later({ op: 'echo', data: 'x'.repeat(Number(new URL(location.href).searchParams.get('bytes') ?? 1)) });
} catch (error) { window.answer = { error: String(error?.message ?? error) }; }
await new Promise(resolve => setTimeout(resolve, 0));
window.announced = announced;
window.ready = true;
`;
const ARTIFACT = `
export async function later(request) { return { op: request.op, length: request.data.length }; }
export function connect({ changed }) { changed('meter'); }
`;

let server;
beforeAll(() => {
  if (unavailable) return;
  const scripts = { '/boot.js': BOOT, '/native.js': file('../../web-js/native.js'), '/native-glue.js': file('../native-glue.js'), '/modules/index.js': ARTIFACT };
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const url = new URL(request.url), policy = POLICIES[Number(url.searchParams.get('policy') ?? 0)] ?? POLICIES[0];
    if (url.pathname === '/') return new Response(PAGE, { headers: { 'content-type': 'text/html', 'content-security-policy': policy } });
    if (scripts[url.pathname]) return new Response(scripts[url.pathname], { headers: { 'content-type': 'text/javascript' } });
    return new Response('missing', { status: 404 });
  } });
});
afterAll(() => { server?.stop(true); });

async function answer(query) {
  const suite = await cdpBrowser(), { cdp } = suite;
  try {
    const { sessionId } = await suite.page();
    await readyPage(cdp, sessionId, `${server.url.origin}/?${query}`);
    const { result } = await cdp.send('Runtime.evaluate', { expression: '({ answer, announced, violations })', returnByValue: true }, sessionId);
    return result.value;
  } finally {
    await suite.close();
  }
}

check("the page module loads and answers native.later under a policy that admits no inline script", async () => {
  for (const policy of POLICIES.keys()) {
    expect(await answer(`policy=${policy}`)).toEqual({ answer: { op: 'echo', length: 1 }, announced: ['meter'], violations: [] });
  }
}, 60000);

check('a request of several megabytes reaches later(request) whole', async () => {
  const bytes = 4 << 20;
  expect((await answer(`bytes=${bytes}`)).answer).toEqual({ op: 'echo', length: bytes });
}, 60000);
