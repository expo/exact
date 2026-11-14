import { beforeAll, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
import { webCompiler } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const fixture = resolve(root, 'target/logic-target-fixture');
let grants;
beforeAll(() => {
  // The same source and calls run in process, through wasmi and in a native library.
  // Its source is logic/tests/fixtures/targets.rs.
  const built = spawnSync('cargo', ['test', '--offline', '-p', 'exact-logic',
    'target_aware_calls_survive_wasm_and_native_modules', '--', '--ignored'],
  { cwd: root, encoding: 'utf8', timeout: 180_000 });
  expect(built.status, built.stdout + built.stderr).toBe(0);
  const input = resolve(fixture, 'grants.txt');
  writeFileSync(input, 'net.fetch local-network development');
  const { cmd, pre } = webCompiler();
  if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const normalized = spawnSync(cmd, [...pre, 'normalize-grants', input], { cwd: root, encoding: 'utf8' });
  expect(normalized.status, normalized.stderr).toBe(0);
  grants = JSON.parse(normalized.stdout);
}, 180_000);

test('Rust module targets survive concurrent resources, mutations and parallel sends in Chrome', async () => {
  let replies = [], waveSize = 6;
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    // Complete each wave backwards; every target asks for a second HTTP reply.
    if (path === '/reply') return new Promise(done => {
      replies.push(done);
      if (replies.length === waveSize) {
        const wave = replies; replies = [];
        for (const reply of wave.reverse()) reply(new Response('reply'));
      }
    });
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    if (path.endsWith('/admission-data.js')) return new Response(
      "import {createGrantSet} from './admission.js';export const rustGrantSet=createGrantSet(" + JSON.stringify(grants) + "),tsGrantSet=rustGrantSet;",
      { headers: { 'content-type': 'text/javascript' } });
    if (path.endsWith('/app.bind.plan')) return new Response(Bun.file(resolve(fixture, 'app.bind.plan')));
    if (path.endsWith('/rust/wasm/app.module.wasm')) return new Response(Bun.file(
      resolve(fixture, 'target/wasm32-unknown-unknown/debug/logic_target_fixture.wasm')));
    const local = resolve(root, '.' + path);
    const file = existsSync(local) ? local : resolve(root, 'host/web', basename(path));
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const { resEach } = await import('/host/web-js/each.js');
      const { install } = await import('/host/web-js/rust-data.js');
      await install(rt.data, { read: 's|s', prefetch: 's|s' });
      const args = [location.origin + '/reply'], landed = [], shown = [], members = rt.sig([1, 2]);
      const parse = rt.data.parse;
      rt.data.parse = (...args) => {
        const answer = parse(...args);
        if (answer.v !== undefined) landed.push(answer.v);
        return answer;
      };
      const slots = [];
      let values;
      rt.mount(root => {
        // List members share a name but have distinct owners.
        values = resEach('member', 'read', members, x => x, () => args, '[s', 's', 'waiting');
        const node = root.appendChild(document.createElement('p'));
        rt.effect(() => { shown.splice(0, shown.length, ...values()); node.textContent = shown.join('\n'); });
        for (let i = 0; i < 3; i++) {
          const slot = rt.sig('bake placeholder', 's');
          slots.push({ slot, mutation: rt.mut('write-' + i, slot, [], 's', i === 2 ? 2 : 0) });
        }
      });
      rt.act(() => {
        for (const i of [1, 0, 2]) rt.M(slots[i].mutation, 'read', args);
        rt.M(slots[2].mutation, 'read', args);
      })();
      globalThis.fixture = { landed, shown, slots, rt, members, values };
    });
    try {
      await page.waitForFunction(() => fixture.landed.length === 6, null, { timeout: 10_000 });
    } catch (error) {
      throw new Error(error.message + '\n' + JSON.stringify(await page.evaluate(() => ({
        landed: fixture.landed, shown: fixture.shown, journal: [...fixture.rt.journal],
        slots: fixture.slots.map(({ slot, mutation }) => [slot(), mutation.p(), mutation.e()]),
      }))));
    }
    const result = await page.evaluate(() => ({
      landed: fixture.landed,
      shown: fixture.shown,
      slots: fixture.slots.map(({ slot, mutation }) => [slot(), mutation.p(), mutation.e()]),
      journal: [...fixture.rt.journal],
    }));
    expect(result.landed.toSorted()).toEqual([
      'Resource(0):1:reply', 'Resource(1):2:reply',
      'Mutation(1):3:reply', 'Mutation(0):4:reply',
      'Sent(2, 1):5:reply', 'Sent(2, 2):6:reply',
    ].toSorted());
    expect(result.shown).toEqual(['Resource(0):1:reply', 'Resource(1):2:reply']);
    expect(result.slots).toEqual([
      ['Mutation(0):4:reply', false, null], ['Mutation(1):3:reply', false, null],
      ['Sent(2, 2):6:reply', false, null],
    ]);
    expect(result.journal.filter(line => /failed|refused/.test(line))).toEqual([]);
    waveSize = 2;
    await page.evaluate(() => {
      const { rt, members, values } = fixture;
      rt.act(() => rt.W(members, [2]))();
      rt.act(() => { rt.W(members, [2, 3]); rt.R(values); })();
    });
    await page.waitForFunction(() => fixture.landed.length === 8, null, { timeout: 10_000 });
    const refreshed = await page.evaluate(() => ({
      shown: fixture.shown, landed: fixture.landed.slice(6), failure: fixture.values.e(),
    }));
    expect(refreshed.landed.map(s => s.split(':')[0]).toSorted()).toEqual(['Resource(1)', 'Resource(2)']);
    expect(refreshed.shown.map(s => s.split(':')[0])).toEqual(['Resource(1)', 'Resource(2)']);
    expect(refreshed.failure).toBe(null);
    // A mutation's prefetch is refused before data.fetch runs, on each send's own ticket.
    const refused = await page.evaluate(() => {
      const { rt } = fixture, slots = [];
      rt.mount(() => {
        for (const mode of [0, 2]) {
          const slot = rt.sig('bake placeholder', 's');
          slots.push({ slot, mutation: rt.mut('prefetch-' + mode, slot, [], 's', mode) });
        }
      });
      rt.act(() => {
        for (const i of [0, 1, 1]) rt.M(slots[i].mutation, 'prefetch', [location.origin + '/reply']);
      })();
      return {
        landed: fixture.landed.slice(8),
        slots: slots.map(({ slot, mutation }) => [slot(), mutation.p(), mutation.e()]),
      };
    });
    const refusal = 'exactPrefetch requires a plain HTTP read';
    expect(refused.landed).toEqual([
      `Mutation(3):9:${refusal}`, `Sent(4, 1):10:${refusal}`, `Sent(4, 2):11:${refusal}`,
    ]);
    expect(refused.slots).toEqual([
      [`Mutation(3):9:${refusal}`, false, null], [`Sent(4, 2):11:${refusal}`, false, null],
    ]);
    expect(errors).toEqual([]);
  } finally {
    for (const reply of replies) reply(new Response('closed', { status: 503 }));
    await browser.close();
    server.stop(true);
  }
}, 30_000);
