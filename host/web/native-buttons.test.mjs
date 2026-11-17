import { expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { chromium } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { serveBuildTree } from './serve.mjs';

const ROOT = resolve(import.meta.dir, '../..');
const compiler = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'), 'debug',
  process.platform === 'win32' ? 'contract.exe' : 'contract');

need('chrome', chromium().unavailable)('the native button showcase keeps its natural heights and scrolls to every control', async () => {
  mkdirSync(resolve(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(ROOT, 'target/native-buttons-'));
  const plan = resolve(dir, 'app.plan'), dist = resolve(dir, 'dist');
  let server, session;
  try {
    const compile = spawnSync(compiler, ['build', resolve(ROOT, 'scripts/fixtures/native-buttons.contract'), '-o', plan], { encoding: 'utf8' });
    expect(compile.status, compile.stderr).toBe(0);
    // This bare plan needs only a manifest; Material Specimens has no data or GPU module to build.
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'material-specimens', '--plan', plan, '--out', dist, '--render', 'none'],
      { cwd: ROOT, encoding: 'utf8' });
    expect(build.status, build.stderr).toBe(0);
    server = createServer((request, response) => serveBuildTree(dist, request, response));
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    session = await open({ host: 'web', app: 'material-specimens', size: [420, 900], url: `http://127.0.0.1:${server.address().port}/` });
    const heights = await session.carrier.evaluate(() => {
      const root = document.querySelector('[data-testid="native-buttons"]');
      const elements = [...root.querySelectorAll('[data-testid]')].filter(el => !el.closest('dialog, [popover]'));
      const constrained = elements.map(el => el.getBoundingClientRect().height);
      const height = root.style.height;
      root.style.height = 'auto';
      const natural = elements.map(el => el.getBoundingClientRect().height);
      root.style.height = height;
      return elements.flatMap((el, i) => Math.abs(constrained[i] - natural[i]) > 0.1
        ? [{ id: el.dataset.testid, constrained: constrained[i], natural: natural[i] }] : []);
    });
    expect(heights).toEqual([]);
    const scroll = await session.carrier.evaluate(() => {
      const root = document.querySelector('[data-testid="native-buttons"]');
      const bounds = root.getBoundingClientRect();
      const buttons = [...root.querySelectorAll('button')].filter(el => !el.closest('dialog, [popover]'));
      const clipped = [];
      for (const button of buttons) {
        button.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' });
        const rect = button.getBoundingClientRect();
        if (rect.height <= 0 || rect.top < bounds.top || rect.bottom > bounds.bottom) clipped.push(button.dataset.testid);
      }
      return { clipped, range: root.scrollHeight - root.clientHeight, offset: root.scrollTop };
    });
    expect(scroll.clipped).toEqual([]);
    expect(scroll.range).toBeGreaterThan(0);
    expect(scroll.offset).toBeGreaterThan(0);
    for (const id of ['defrost-front', 'control-tile', 'subtitle', 'check-update', 'narrow', 'g1', 'g2', 'active-properties']) {
      await session.tap(id);
    }
    expect((await session.state()).slots.presses).toBe(7);
    expect((await session.state()).slots.fading).toBe(true);
    await session.tap('g3');
    await session.tap('period-Month');
    expect((await session.state()).slots).toMatchObject({ fading: false, period: 'Month' });
    await session.tap('tint-Day');
    expect((await session.state()).slots.period).toBe('Day');
    await session.tap('open-dialog');
    await session.tap('confirm-action');
    expect((await session.state()).slots.presses).toBe(9);
    await session.tap('open-popover');
    expect((await session.state()).slots.presses).toBe(10);
    expect(await session.carrier.evaluate(() => document.getElementById('button-popover').matches(':popover-open'))).toBe(true);
  } finally {
    await session?.close();
    if (server) await new Promise(resolve => server.close(resolve));
    rmSync(dir, { recursive: true, force: true });
  }
}, 120_000);
