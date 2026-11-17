// LLP 1168.000 D2: the chat pattern (docs/patterns/chat) keeps the reader's place when an older page lands above
// them at the list's very top: the window's old first message (`anchorKey`) is back at the port's top. Checked on
// the JS target in Chrome, from the driver's layout frames and the app's own state. It checks the behaviour, not the
// mechanism: on the web it also holds without the pattern's `scrollIntoView` (2026-11-14). A page that lands while
// the list is inside the band but not at its top is the list's own anchor's (D1.2), and is not checked here.
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join, resolve } from 'node:path';
import { runTests } from '../../../scripts/agent-test.mjs';
import { serveBuildTree } from '../serve.mjs';

const ROOT = resolve(import.meta.dir, '../../..');

// The list, at rest a port and a half below its top (outside the band: no page), is scrolled to its very top. A page
// is asked for and lands.
async function land(s) {
  await s.clock('settle');
  const view = async () => {
    const nodes = (await s.tree()).nodes, layout = (await s.layout()).nodes;
    const box = (testId) => layout.find((n) => n.id === nodes.find((v) => v.props?.testId === testId)?.id);
    return { nodes, box };
  };
  const first = await view(), list = first.nodes.find((v) => v.props?.testId === 'transcript').id, port = first.box('transcript');
  const scroll = async (y) => { await s.carrier.evaluate(`exact.views.get(${list}).scrollTop = ${Math.round(y)}`); await s.clock('settle'); };
  await scroll(1.5 * port.h);
  const still = (await s.state()).slots;
  await scroll(0);
  await s.clock('data');
  await s.clock('settle');
  const after = await view(), slots = (await s.state()).slots;
  return { still, slots, port, anchor: after.box(`message-${slots.anchorKey}`)?.y };
}

test("an older page that lands at the chat pattern's very top keeps the reader's first message at the top", async () => {
  const dir = mkdtempSync(join(ROOT, 'target/chat-anchor-'));
  const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'anchor.test.contract');
  for (const name of ['app.contract', 'app.ts']) cpSync(join(ROOT, 'docs/patterns/chat', name), join(dir, name));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Chat', app: { id: 'test.exact.chat-anchor', name: 'Chat' }, host: {}, deploy: {} }));
  writeFileSync(file, 'test "at the top"\n  clock data\n');
  const compile = spawnSync(join(process.env.CARGO_TARGET_DIR ?? join(ROOT, 'target'), 'debug', 'contract'), ['build', contract, '-o', plan], { cwd: ROOT, encoding: 'utf8' });
  expect(compile.status, compile.stderr).toBe(0);
  const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'chat', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: ROOT, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir } });
  expect(build.status, build.stderr).toBe(0);
  const server = createServer((request, response) => serveBuildTree(dist, request, response));
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  let seen;
  try {
    const r = await runTests({ host: 'web', file, app: dir, url: `http://127.0.0.1:${server.address().port}/`, atEnd: async (s) => {
      seen = await land(s);
      return {};
    } });
    expect(r.results[0].failures).toEqual([]);
  } finally {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  }
  const { still, slots, port, anchor } = seen, said = JSON.stringify({ still: still.cursor, cursor: slots.cursor, anchorKey: slots.anchorKey, anchor, port: port.y });
  // At rest outside the band nothing was asked for; at the top the page before the newest was, and it landed.
  expect(still.cursor, said).toBe('');
  expect(slots.cursor, said).toBe('400');
  expect(slots.pendingStart, said).toBe('');
  // The newest window's first message (451 of 600) is back at the port's top, under its day pill at most.
  expect(slots.anchorKey, said).toBe('451');
  expect(anchor, said).toBeDefined();
  expect(Math.abs(anchor - port.y), said).toBeLessThanOrEqual(30);
}, 180_000);
