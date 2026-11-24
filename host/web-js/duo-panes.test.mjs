import { test, expect } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import process from 'node:process';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

test('Duo panes retain state and avoid book and laptop hinges in both reading directions', async () => {
  const root = resolve(import.meta.dirname, '../..'), dir = mkdtempSync(resolve(root, 'target/duo-panes-test-'));
  let suite;
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'duo-lab', '--out', dir, '--render', 'none'], {
    cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe',
  });
  let server;
  try {
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dir, path === '/' ? 'index.html' : path.slice(1))));
    } });
    suite = await suiteBrowser();
    const page = await suite.newPage({ viewport: { width: 1280, height: 800 } });
    const errors = [];
    page.on('pageerror', e => errors.push(String(e)));
    await page.goto(server.url.href);
    await page.getByTestId('tab-panes').click();
    expect(await page.getByTestId('panes-split').count()).toBe(0);
    expect(await page.getByTestId('panes-policy').textContent()).toContain('require the native iOS app');
    await page.getByTestId('panes-workspace-draft').fill('Keep this draft');
    await page.getByTestId('panes-activity-increment').click();
    await page.evaluate(() => { globalThis.originalPanes = ['workspace', 'activity'].map(name => document.querySelector(`[data-testid="panes-${name}-pane"]`)); });
    const cdp = await page.context().newCDPSession(page);
    for (const direction of ['ltr', 'rtl']) {
      await page.evaluate(direction => { document.documentElement.dir = direction; }, direction);
      for (const [width, height, orientation, offset] of [[1280, 800, 'vertical', 620], [1280, 800, 'vertical', 400], [1280, 800, 'vertical', 840], [800, 1000, 'horizontal', 480]]) {
        await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false,
          displayFeature: { orientation, offset, maskLength: 40 } });
        const paneWidth = Math.min(offset, width - offset - 40);
        await page.waitForFunction(([axis, expected, width]) => {
          const pane = document.querySelector('[data-testid="panes-activity-pane"]');
          const rect = pane.getBoundingClientRect();
          return getComputedStyle(pane).position === 'absolute' && Math.abs(rect[axis] - expected) < 1 && (axis !== 'left' || Math.abs(rect.width - width) < 1);
        }, [orientation === 'vertical' ? 'left' : 'top', orientation === 'vertical' ? (direction === 'rtl' ? 0 : width - paneWidth) : offset + 40, paneWidth]);
        const bounds = await page.evaluate(() => globalThis.originalPanes.map((pane, i) => {
          const r = pane.getBoundingClientRect();
          return { retained: pane === document.querySelector(`[data-testid="panes-${i ? 'activity' : 'workspace'}-pane"]`), left: r.left, right: r.right, top: r.top, bottom: r.bottom };
        }));
        expect(bounds.every(b => b.retained)).toBe(true);
        const start = orientation === 'vertical' ? 'left' : 'top', end = orientation === 'vertical' ? 'right' : 'bottom';
        const first = orientation === 'vertical' && direction === 'rtl' ? 1 : 0;
        expect(bounds[first][end]).toBeLessThanOrEqual(offset + 1);
        expect(bounds[1 - first][start]).toBeGreaterThanOrEqual(offset + 39);
        expect(await page.getByTestId('panes-workspace-echo').textContent()).toBe('Keep this draft');
        expect(await page.getByTestId('panes-activity-count').textContent()).toBe('1');
      }
    }
    await page.evaluate(() => { document.documentElement.dir = 'ltr'; });
    await cdp.send('Emulation.setSafeAreaInsetsOverride', { insets: { top: 64, left: 40, right: 24, bottom: 32 } });
    const title = await page.getByTestId('panes-workspace-pane').getByRole('heading', { level: 1 }).boundingBox();
    const activity = await page.getByTestId('panes-activity-pane').getByRole('heading', { name: 'Activity', exact: true }).boundingBox();
    expect(title.y).toBeGreaterThanOrEqual(80);
    expect(title.x).toBeGreaterThanOrEqual(56);
    expect(activity.x).toBeGreaterThanOrEqual(56);
    expect(activity.x + activity.width).toBeLessThanOrEqual(760);
    // On a book fold the panes pad their sides by the route's insets and not the browser's, which are 0 here: their
    // content keeps 16 CSS pixels inside insets of 40 and 24
    await cdp.send('Emulation.setSafeAreaInsetsOverride', { insets: { top: 0, left: 0, right: 0, bottom: 0 } });
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false,
      displayFeature: { orientation: 'vertical', offset: 620, maskLength: 40 } });
    await page.evaluate(() => {
      document.documentElement.style.setProperty('--exact-safe-area-inset-left', '40px');
      document.documentElement.style.setProperty('--exact-safe-area-inset-right', '24px');
    });
    // A 90-character word in the echo wraps onto several lines inside its pane, folded or flat
    await page.getByTestId('panes-activity-draft').fill('W'.repeat(90));
    const echo = () => page.evaluate(() => {
      const pane = document.querySelector('[data-testid="panes-activity-pane"]'), range = document.createRange();
      range.selectNodeContents(document.querySelector('[data-testid="panes-activity-echo"]'));
      return { overflow: pane.scrollWidth - pane.clientWidth, wrapped: range.getClientRects().length > 1 };
    });
    for (const direction of ['ltr', 'rtl']) {
      await page.evaluate(direction => { document.documentElement.dir = direction; }, direction);
      const [left, right] = direction === 'rtl' ? ['activity', 'workspace'] : ['workspace', 'activity'];
      await page.waitForFunction(right => Math.abs(document.querySelector(`[data-testid="panes-${right}-pane"]`).getBoundingClientRect().left - 660) < 1
        && document.querySelector('[data-testid="panes-activity-echo"]').textContent.length === 90, right);
      const edges = await page.evaluate(([left, right]) => ({
        left: document.querySelector(`[data-testid="panes-${left}-draft"]`).getBoundingClientRect().left,
        right: document.querySelector(`[data-testid="panes-${right}-draft"]`).getBoundingClientRect().right,
      }), [left, right]);
      expect({ direction, ...edges, ...await echo() }).toEqual({ direction, left: 56, right: 1240, overflow: 0, wrapped: true });
    }
    await page.evaluate(() => { document.documentElement.dir = 'ltr'; });
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
    await page.waitForFunction(() => getComputedStyle(document.querySelector('[data-testid="panes-activity-pane"]')).position === 'relative');
    expect(await echo()).toEqual({ overflow: 0, wrapped: true });
    await page.getByTestId('panes-activity-draft').fill('');
    await page.evaluate(() => {
      document.documentElement.style.removeProperty('--exact-safe-area-inset-left');
      document.documentElement.style.removeProperty('--exact-safe-area-inset-right');
    });
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 800, height: 1000, deviceScaleFactor: 1, mobile: false });
    await page.getByTestId('panes-next').click();
    await page.getByTestId('pushed-next').click();
    expect(await page.getByTestId('panes-workspace-echo').textContent()).toBe('Keep this draft');
    await page.getByTestId('tab-fold').click();
    expect(await page.getByTestId('fact-h').textContent()).toBe('1');
    expect(errors).toEqual([]);
    // The runtime journals a style the browser refuses without throwing
    expect(await page.evaluate(() => exact.journal.filter(line => line.includes(' is not a value it takes')))).toEqual([]);
  } finally { await suite?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 180000);
