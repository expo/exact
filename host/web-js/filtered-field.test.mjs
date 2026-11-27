import { expect } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { renderedPages } from './rendered-pages.mjs';
import { serveBuildTree, serveStatic } from '../web/serve.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { engineTests, need } from '../../scripts/coverage.mjs';

const engine = await engineTests(['chromium']);
const fixture = fileURLToPath(new URL('./conformance/filtered-field.contract', import.meta.url));
const source = readFileSync(fixture, 'utf8');

async function check(dist, serve, plan) {
  const server = createServer((req, res) => serve(dist, req, res));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.route('https://example.test/field-answer', route => route.fulfill({ json: { text: 'answer' }, headers: { 'access-control-allow-origin': '*' } }));
    if (plan) {
      const { exact, app, plan: entry } = JSON.parse(readFileSync(resolve(dist, 'exact.json'), 'utf8'));
      const sha256 = createHash('sha256').update(plan).digest('hex');
      await page.route('**/exact.json', route => route.fulfill({
        json: { exact, app, plan: { ...entry, url: './app.plan', bytes: plan.length, sha256 } },
      }));
      await page.route('**/app.plan', route => route.fulfill({ body: plan }));
    }
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const field = id => page.locator(`[data-testid="${id}"]`);
    const position = (f, a, b = a) => f.evaluate((e, [a, b]) => {
      e.focus();
      e.setSelectionRange(a, b);
    }, [a, b]);
    const shown = f => f.evaluate(e => [e.value, e.selectionStart, e.selectionEnd]);
    await page.waitForFunction(() => document.querySelector('[data-testid="resource"]')?.value === 'answer');
    await field('resource').fill('draft');
    expect(await field('resource').inputValue()).toBe('draft');
    for (const row of ['row-a', 'row-b']) {
      await field(row).fill('x');
      expect(await field(row).inputValue()).toBe('A');
    }
    const digits = field('digits');
    await position(digits, 2);
    await page.keyboard.type('a');
    expect(await shown(digits)).toEqual(['40', 2, 2]);
    await position(digits, 1);
    await page.keyboard.type('a');
    expect(await shown(digits)).toEqual(['40', 1, 1]);
    await position(digits, 0, 1);
    await page.keyboard.type('a');
    expect(await shown(digits)).toEqual(['0', 0, 0]);
    await page.keyboard.press('ControlOrMeta+z');
    // Chromium's undo leaves the accepted replacement and its collapsed selection.
    expect(await shown(digits)).toEqual(['0', 0, 0]);
    expect(await digits.inputValue()).toBe(await field('accepted').textContent());

    // Paste from another field in the same headless browser.
    const free = field('free');
    await free.fill('7x');
    await position(free, 0, 2);
    await page.keyboard.press('ControlOrMeta+c');
    await digits.fill('40');
    await position(digits, 1);
    await page.keyboard.press('ControlOrMeta+v');
    expect(await shown(digits)).toEqual(['470', 2, 2]);
    for (const id of ['area', 'search', 'password']) {
      const f = field(id);
      await position(f, 1);
      await page.keyboard.type('a');
      expect(await shown(f)).toEqual(['470', 1, 1]);
    }
    const lines = field('lines');
    await position(lines, 3);
    await page.keyboard.type('x');
    expect(await shown(lines)).toEqual(['a\nbc', 3, 3]);
    const backward = await digits.evaluate(e => {
      e.focus();
      e.value = 'ab470cd';
      e.setSelectionRange(1, 6, 'backward');
      e.dispatchEvent(new InputEvent('input', { bubbles: true }));
      return [e.value, e.selectionStart, e.selectionEnd, e.selectionDirection];
    });
    expect(backward).toEqual(['470', 1, 3, 'backward']);
    const cap = field('short');
    await position(cap, 3);
    await page.keyboard.type('d');
    expect(await shown(cap)).toEqual(['abc', 3, 3]);

    const sibling = field('sibling');
    await sibling.fill('uncommitted');
    await position(sibling, 3);
    await field('record').fill('discard');
    await field('record').evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    expect(await field('assignments').textContent()).toBe('1');
    expect(await field('record').inputValue()).toBe('A');
    expect(await shown(sibling)).toEqual(['uncommitted', 3, 3]);
    const echo = field('echo');
    await echo.fill('abc');
    await position(echo, 1);
    await page.keyboard.type('x');
    expect(await shown(echo)).toEqual(['axbc', 2, 2]);
    await position(echo, 0);
    await field('set-echo').evaluate(e => e.click());
    // A button's update keeps the target's existing setter and caret behavior.
    expect(await shown(echo)).toEqual(['A', plan ? 0 : 1, plan ? 0 : 1]);

    await field('conditional').fill('half');
    expect(await field('conditional').inputValue()).toBe('half');
    await field('guard').fill('rejected');
    expect(await field('guard').inputValue()).toBe('rejected');
    await field('quantity').fill('-');
    expect(await field('quantity').inputValue()).toBe('-');
    await page.keyboard.press('Enter');
    expect(await field('quantity').inputValue()).toBe('0');
    await field('commit-only').fill('abcd');
    expect(await field('commit-only').inputValue()).toBe('abcd');
    await field('pick').selectOption('b');
    expect(await field('pick').inputValue()).toBe('b');
    for (const [id, value] of [['date', '2026-12-01'], ['time', '13:30'], ['range', '41']]) {
      await field(id).evaluate((e, v) => {
        e.value = v;
        e.dispatchEvent(new Event('input', { bubbles: true }));
        e.dispatchEvent(new Event('change', { bubbles: true }));
      }, value);
      expect(await field(id).inputValue()).toBe(value);
    }
    for (const id of ['box', 'toggle', 'radio-b']) {
      await field(id).click();
      expect(await field(id).isChecked()).toBe(false);
    }
    expect(await field('radio-a').isChecked()).toBe(true);

    await field('flip').fill('ignored');
    expect(await field('flip').inputValue()).toBe('half');

    // Some input methods end a cancelled composition without a final input.
    await echo.evaluate(e => {
      const input = value => {
        e.value = value;
        e.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true }));
      };
      e.focus();
      e.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
      input('marked');
      document.querySelector('[data-testid="set-echo"]').click();
      input('A');
      e.value = 'cancelled';
      e.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
    });
    await echo.evaluate(() => new Promise(resolve => setTimeout(resolve, 0)));
    expect(await echo.inputValue()).toBe('cancelled');

    let deliver;
    const reply = new Promise(resolve => { deliver = resolve; });
    await page.route('https://example.test/filtered-field', async route => {
      await reply;
      await route.fulfill({ json: { text: 'saved' }, headers: { 'access-control-allow-origin': '*' } });
    });
    const saving = field('async');
    await saving.fill('draft');
    expect(await saving.inputValue()).toBe('draft');
    deliver();
    await page.waitForFunction(() => document.querySelector('[data-testid="async"]').value === 'saved');

    const cdp = await page.context().newCDPSession(page);
    const mark = text => cdp.send('Input.imeSetComposition', { text, selectionStart: text.length, selectionEnd: text.length });
    for (const cancel of [false, true]) {
      await position(digits, 1);
      await mark('a');
      expect(await digits.inputValue()).toBe('4a70');
      if (cancel) await mark('');
      else await cdp.send('Input.insertText', { text: 'a' });
      await page.waitForFunction(() => document.querySelector('[data-testid="digits"]').value === '470');
      expect(await shown(digits)).toEqual(['470', 1, 1]);
    }
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
}

engine.chromium('a JS text field shows what its input action assigned', () =>
  renderedPages(source, (_, __, dist) => check(dist, serveBuildTree)), 180000);

const wasm = process.env.EXACT_WEB_DIST;
need('fixture', wasm ? null : 'name a whole-linked wasm build with EXACT_WEB_DIST', { test: engine.chromium })(
  'a wasm text field shows what its input action assigned', async () => {
    const temp = mkdtempSync(resolve(tmpdir(), 'exact-filtered-field-'));
    try {
      const plan = resolve(temp, 'field.plan');
      const target = process.env.CARGO_TARGET_DIR ?? resolve(import.meta.dir, '../../target');
      const app = JSON.parse(readFileSync(resolve(wasm, 'exact.json'), 'utf8')).app;
      writeFileSync(resolve(temp, 'app.json'), JSON.stringify({ app, host: { web: {} } }));
      writeFileSync(resolve(temp, 'app.contract'), source);
      execFileSync(resolve(target, 'debug', process.platform === 'win32' ? 'contract.exe' : 'contract'), ['build', resolve(temp, 'app.contract'), '-o', plan]);
      await check(wasm, serveStatic, readFileSync(plan));
    } finally { rmSync(temp, { recursive: true, force: true }); }
  }, 180000);
