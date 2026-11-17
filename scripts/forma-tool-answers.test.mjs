// `forma tool --via`'s probe of a dev loop (scripts/forma-server.mjs
// `toolAnswers`): a token probe that times out (2 s) is tried again, so a dev
// loop under load that answers late is still reached; nothing listening is
// said at once, and a loop that never answers is refused after the tries
// (QUEUE "`forma tool --via` gives a dev loop 2 s to answer").
import { expect, test } from 'bun:test';
import { createServer } from 'node:net';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { toolAnswers } from './forma-server.mjs';

const free = () => new Promise(ok => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => ok(port)); }); });

test('the tool command prints a former protocol refusal without a stack at either immediate probe', () => {
  const app = mkdtempSync(join(tmpdir(), 'forma-tool-former-'));
  try {
    writeFileSync(join(app, 'app.forma'), 'forma app\n');
    for (const via of [false, true]) {
      const source = `
        import { mock } from 'bun:test';
        const appModule = await import('./scripts/app.mjs'), launcher = await import('./scripts/agent-launch.mjs'), serve = await import('./host/web/serve.mjs');
        mock.module('./scripts/app.mjs', () => ({ ...appModule, resolveApp: () => ({ dir: ${JSON.stringify(app)} }) }));
        mock.module('./scripts/agent-launch.mjs', () => ({ ...launcher, webChanges: () => ({ all: [] }) }));
        mock.module('./host/web/serve.mjs', () => ({ ...serve, builtAppMatches: async () => true, jsTargetBuild: () => true }));
        const { toolCommand, TOOL_ORIGIN } = await import('./scripts/forma-server.mjs');
        globalThis.fetch = async url => {
          const at = new URL(url);
          return Response.json(at.pathname === '/__forma/token' ? { token: 't' } : { ok: {
            app: ${JSON.stringify(app)}, epoch: 'e1', protocol: ${via} && at.origin === TOOL_ORIGIN ? 'forma/1.18' : 'tamber/1.18'
          } });
        };
        process.exitCode = await toolCommand([${JSON.stringify(app)}, '--no-open', ...(${via} ? ['--via', 'http://127.0.0.2:43210'] : [])]);
      `;
      const r = spawnSync(process.execPath, ['-e', source], { cwd: resolve(import.meta.dir, '..'), encoding: 'utf8' });
      expect(r.status, r.stderr).toBe(1);
      expect(r.stderr).toBe('forma tool: rename protocol tamber/1.18 to forma/1.18\n');
    }
  } finally { rmSync(app, { recursive: true, force: true }); }
});

// A stand-in design server: its token answered after `late(n)` ms on the n-th ask, its hello after `slowHello` ms.
const loop = (late, slowHello = 0) => {
  let asked = 0;
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', async fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/__forma/token') { await Bun.sleep(late(++asked)); return Response.json({ token: 't' }); }
    if (path === '/__forma') { await Bun.sleep(slowHello); return Response.json({ ok: { app: '/apps/x', epoch: 'e1' } }); }
    return new Response('', { status: 404 });
  } });
  return { server, origin: `http://127.0.0.1:${server.port}`, asked: () => asked };
};

test('a dev loop that answers its first token probe late is reached on a later try; one probe alone is not', async () => {
  const slow = loop(n => (n === 1 ? 3000 : 0));
  try {
    expect(await toolAnswers(slow.origin, 5)).toEqual({ app: '/apps/x', epoch: 'e1' });
    expect(slow.asked()).toBe(2);
  } finally { slow.server.stop(true); }
  const once = loop(n => (n === 1 ? 3000 : 0));
  try { expect(await toolAnswers(once.origin)).toBe(null); } finally { once.server.stop(true); }
}, 30_000);

test('nothing listening is said at once, tries or not; a loop that never answers is refused after them', async () => {
  // Refused: one attempt, however many tries are allowed (the fetches to it counted).
  const port = await free(), refused = `http://127.0.0.1:${port}`, real = globalThis.fetch;
  let attempts = 0;
  globalThis.fetch = (url, ...rest) => { if (String(url).startsWith(refused)) attempts++; return real(url, ...rest); };
  try { expect(await toolAnswers(refused, 5)).toBe(null); } finally { globalThis.fetch = real; }
  expect(attempts).toBe(1);
  const mute = loop(() => 60_000), t1 = Date.now();
  try {
    expect(await toolAnswers(mute.origin, 3)).toBe(null);
    expect([mute.asked(), Date.now() - t1 >= 5900]).toEqual([3, true]);
  } finally { mute.server.stop(true); }
}, 60_000);

test("a dev loop that gives its token at once but its hello late (a loaded loop starting its design server) is still reached through --via; the tool's own check keeps its 10 s", async () => {
  const slow = loop(() => 0, 12_000);
  try {
    expect(await toolAnswers(slow.origin, 5)).toEqual({ app: '/apps/x', epoch: 'e1' });
    expect(await toolAnswers(slow.origin)).toBe(null);
  } finally { slow.server.stop(true); }
}, 60_000);
