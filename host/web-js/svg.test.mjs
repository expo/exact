// Build exact-web-js once before running this test (docs/testing.md).
import { test, expect } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { webkit } from 'playwright-core';
import { decodePng } from '../../scripts/png.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

test('dynamic SVG attribute lengths are refused before reaching the DOM', async () => {
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/svg-lengths-'));
  let server, browser;
  try {
    const compiled = spawnSync(resolve(root, `target/debug/exact-web-js${EXE}`), [
      'js', resolve(root, 'runner/tests/fixtures/svg-lengths.contract'), '-o', dir,
    ], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<div id="exact-root"></div>
        <script type="module">import app from '/app.js';app();</script>`,
      { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href);
    const field = page.getByTestId('value');
    await field.waitFor({ state: 'attached' });
    expect(await page.getByTestId('invalid-literal-line').evaluate(e =>
      ['x1', 'y1', 'x2'].map(name => e.getAttribute(name)))).toEqual([null, null, null]);
    expect(await page.getByTestId('invalid-literal-text').getAttribute('x')).toBeNull();
    const initialLines = await page.evaluate(async () => (await import('/rt.js')).journal);
    for (const refused of ['invalid x1 value ""; unset', 'invalid y1 value "10px 20px"; unset',
      'invalid x2 value "1e9999"; unset', 'invalid textX value ""; unset']) {
      expect(initialLines.some(line => line.includes('runtime-svg-length:') && line.endsWith(refused)), refused).toBe(true);
    }
    for (const [value, valid] of [['2px', true], ['2em', false], ['3CM', true],
      ['calc(1px + 2px)', false], ['50%', true], ['env(safe-area-inset-top)', false],
      ['1e2pt', true], ['var(--x)', false], ['10px, 2em', false], ['0', true]]) {
      await field.fill(value);
      const values = await page.evaluate(() => {
        const attrs = { line: ['x1', 'y1', 'x2', 'y2'], linear: ['x1', 'y1', 'x2', 'y2'],
          radial: ['fx', 'fy', 'fr'], marker: ['markerWidth', 'markerHeight', 'refX', 'refY'],
          text: ['x', 'y', 'dx', 'dy'], span: ['x', 'y', 'dx', 'dy'] };
        return Object.entries(attrs).flatMap(([id, names]) => names.map(name =>
          document.querySelector(`[data-testid="${id}"]`).getAttribute(name)));
      });
      expect(values, value).toEqual(Array(23).fill(valid ? value : null));
      expect(await page.evaluate(() => [['marker', 'orient'], ['offset', 'offset'], ['primitive', 'dx'], ['primitive', 'dy']]
        .map(([id, name]) => document.querySelector(`[data-testid="${id}"]`).getAttribute(name))))
        .toEqual(Array(4).fill(value));
      if (!valid) {
        const lines = await page.evaluate(async () => (await import('/rt.js')).journal);
        expect(lines.some(line => line.includes('runtime-svg-length') && line.includes(value)), value).toBe(true);
      }
    }
    for (const value of ['1 2px, 3%', 'center', 'left', 'right', 'top', 'bottom', '1in', '1cm', '1mm', '1pt', '1pc']) {
      await field.fill(value);
      const values = await page.evaluate(() => {
        const attrs = { line: ['x1'], text: ['x', 'y', 'dx', 'dy'], marker: ['refX', 'refY'] };
        return Object.entries(attrs).flatMap(([id, names]) => names.map(name =>
          document.querySelector(`[data-testid="${id}"]`).getAttribute(name)));
      });
      const expected = value === '1 2px, 3%' ? [null, value, value, value, value, null, null]
        : value === 'center' ? [null, null, null, null, null, value, value]
          : ['left', 'right'].includes(value) ? [null, null, null, null, null, value, null]
            : ['top', 'bottom'].includes(value) ? [null, null, null, null, null, null, value]
              : Array(7).fill(value);
      expect(values, value).toEqual(expected);
    }
    for (const unit of ['em', 'px', 'vh', '%']) {
      await page.getByTestId('unit').fill(unit);
      expect(await page.getByTestId('assembled').evaluate(e => [e.getAttribute('x1'), e.getAttribute('y1')]))
        .toEqual(Array(2).fill(['px', '%'].includes(unit) ? `2${unit}` : null));
    }
    expect(errors).toEqual([]);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);

test('compiled SVG geometry bindings render, change, unset and preserve zero', async () => {
  const root = resolve(import.meta.dir, '../..');
  const target = resolve(root, 'target');
  mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(resolve(target, 'svg-bindings-'));
  const sheet = readFileSync(resolve(root, 'host/web/index.html'), 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1];
  let server, browser;
  try {
    const compiled = spawnSync(resolve(target, `debug/exact-web-js${EXE}`), [
      'js', resolve(import.meta.dir, 'conformance/svg-geometry.contract'), '-o', dir,
    ], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${sheet}</style><link rel="stylesheet" href="/app.css">
        <div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`,
      { headers: { 'content-type': 'text/html' } });
      // A module that re-exports the host's own reaches it through `../web/`.
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href);
    expect(errors).toEqual([]);
    const nested = page.getByTestId('nested');
    await nested.waitFor({ state: 'attached' });
    const paint = () => page.evaluate(() => ['bound-paint', 'inherited-paint', 'own-paint'].map(id => {
      const element = document.querySelector(`[data-testid="${id}"]`);
      const style = getComputedStyle(element);
      return [style.fill, style.stroke, style.strokeWidth];
    }));
    expect(await paint()).toEqual([['rgb(255, 0, 0)', 'none', '1px'], ['rgb(255, 0, 0)', 'none', '1px'], ['rgb(0, 255, 0)', 'rgb(18, 52, 86)', '2px']]);
    expect(await page.getByTestId('viewport-static').evaluate(e => [e.getBBox().width, e.getBBox().height])).toEqual([80, 60]);
    const hairline = page.getByTestId('hairline');
    const height = element => element.evaluate(e => e.getBoundingClientRect().height);
    expect(await height(hairline)).toBe(1);
    expect(await height(page.getByTestId('hairline-static'))).toBe(1);
    expect(await hairline.getAttribute('height')).toBeNull();
    expect(await hairline.evaluate(e => e.style.height)).toBe('var(--exact-hairline, 1px)');
    await page.getByTestId('hairline-toggle').click();
    expect(await height(hairline)).toBe(10);
    expect(await paint()).toEqual([['rgb(0, 0, 255)', 'none', '1px'], ['rgb(0, 0, 255)', 'none', '1px'], ['rgb(255, 255, 0)', 'rgb(18, 52, 86)', '2px']]);
    expect(await hairline.getAttribute('height')).toBe('10px');
    await page.getByTestId('hairline-toggle').click();
    expect(await height(hairline)).toBe(1);
    expect((await paint()).map(value => value[0])).toEqual(['rgb(255, 0, 0)', 'rgb(255, 0, 0)', 'rgb(0, 255, 0)']);
    expect(errors).toEqual([]);
    expect(await page.getByTestId('use').evaluate(e => {
      const box = e.getBoundingClientRect();
      return [e.getAttribute('width'), e.getAttribute('height'), box.width, box.height];
    })).toEqual(['25vh', '25vh', 150, 150]);
    const envGeometry = () => page.evaluate(() => ['environment-static', 'environment-bound'].map(id => {
      const e = document.querySelector(`[data-testid="${id}"]`);
      return { attribute: e.getAttribute('height'), height: e.querySelector('rect').getBBox().height };
    }));
    expect(await envGeometry()).toEqual([{ attribute: null, height: 400 }, { attribute: null, height: 400 }]);
    expect(await page.getByTestId('environment-gradient').getAttribute('cx')).toBeNull();
    await page.getByTestId('fallback').click();
    expect(await envGeometry()).toEqual([{ attribute: null, height: 400 }, { attribute: null, height: 400 }]);
    expect(await page.getByTestId('environment-gradient').getAttribute('cx')).toBeNull();
    await page.getByTestId('fallback').click();
    expect(await envGeometry()).toEqual([{ attribute: null, height: 400 }, { attribute: null, height: 400 }]);
    expect(await page.getByTestId('environment-bound').evaluate(e => e.style.height)).toContain('env(');
    const cdp = await page.context().newCDPSession(page);
    for (const [scale, expected] of [[1.25, 1], [2, 0.5], [3, 1 / 3], [1, 1]]) {
      // CDP needs a viewport resize to invalidate resolution media queries.
      await cdp.send('Emulation.setDeviceMetricsOverride', { width: 800 + Math.round(scale * 10), height: 600, deviceScaleFactor: scale, mobile: false });
      await page.evaluate(() => new Promise(requestAnimationFrame));
      expect(await hairline.getAttribute('height')).toBeNull();
      expect(await height(hairline)).toBeCloseTo(expected, 1);
      for (const id of ['hairline-static', 'hairline-pattern']) {
        expect(await page.getByTestId(id).getAttribute('height')).toBe('1');
      }
      await page.getByTestId('hairline-toggle').click();
      expect(await hairline.getAttribute('height')).toBe('10px');
      expect(await hairline.evaluate(e => e.style.height)).toBe('');
      await page.getByTestId('hairline-toggle').click();
      await page.evaluate(() => new Promise(requestAnimationFrame));
      expect(await hairline.getAttribute('height')).toBeNull();
      expect(await height(hairline)).toBeCloseTo(expected, 1);
    }
    for (const [width, sharp, ellipseWidth] of [[200, true, 0], [300, true, 0], [400, true, 20], [800, true, 100], [900, true, 120], [1000, false, 140], [800, true, 100]]) {
      await page.setViewportSize({ width, height: 600 });
      expect(await page.getByTestId('calculated-rect').evaluate(e => e.isPointInFill(new DOMPoint(1, 1)))).toBe(sharp);
      expect(await page.getByTestId('calculated-ellipse').evaluate(e => e.getBBox().width)).toBe(ellipseWidth);
    }
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 800, height: 600, deviceScaleFactor: 1, mobile: false,
      displayFeature: { orientation: 'vertical', offset: 400, maskLength: 0 } });
    await page.evaluate(() => dispatchEvent(new Event('resize')));
    expect(await envGeometry()).toEqual([
      { attribute: null, height: 400 },
      { attribute: null, height: 400 },
    ]);
    expect(await page.getByTestId('environment-gradient').getAttribute('cx')).toBeNull();
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 800, height: 600, deviceScaleFactor: 1, mobile: false });
    expect(await envGeometry()).toEqual([{ attribute: null, height: 400 }, { attribute: null, height: 400 }]);
    expect(await page.getByTestId('environment-gradient').getAttribute('cx')).toBeNull();
    const geometry = () => nested.evaluate(e => ({
      attribute: e.getAttribute('height'),
      height: e.querySelector('rect').getBBox().height,
    }));
    const attributes = async (extent, offset, foreignExtent = extent, useExtent = extent) => {
      for (const id of ['mask', 'pattern', 'filter', 'primitive', 'foreign', 'use']) {
        const size = id === 'foreign' ? foreignExtent : id === 'use' ? useExtent : extent;
        expect(await page.getByTestId(id).evaluate(e =>
          ['x', 'y', 'width', 'height'].map(name => e.getAttribute(name))
        )).toEqual([offset, offset, size, size]);
      }
      expect(await page.getByTestId('gradient').evaluate(e =>
        ['cx', 'cy', 'r'].map(name => e.getAttribute(name))
      )).toEqual([extent, extent, extent]);
      expect(await nested.evaluate(e => [e.getAttribute('x'), e.getAttribute('y')]))
        .toEqual([offset, offset]);
    };
    expect(await geometry()).toEqual({ attribute: '25vh', height: 150 });
    await attributes('25vh', '10');
    await page.getByTestId('resize').click();
    expect(await geometry()).toEqual({ attribute: '50vh', height: 300 });
    await attributes('50vh', '20');
    await page.getByTestId('omit').click();
    expect(await geometry()).toEqual({ attribute: null, height: 400 });
    await attributes(null, null);
    await page.getByTestId('omit').click();
    expect(await geometry()).toEqual({ attribute: '50vh', height: 300 });
    await page.getByTestId('zero').click();
    expect(await geometry()).toEqual({ attribute: '0', height: 0 });
    await attributes('0', '0', '0', '25vh');
    await page.getByTestId('zero').click();
    expect(await geometry()).toEqual({ attribute: '50vh', height: 300 });
    await page.getByTestId('auto').click();
    expect(await geometry()).toEqual({ attribute: null, height: 400 });
    await attributes(null, '20', '10px');
    expect(await page.getByTestId('environment-bound').getAttribute('height')).toBe('10px');
    expect(await page.getByTestId('environment-gradient').getAttribute('cx')).toBe('10px');
    expect(await page.getByTestId('use').getAttribute('height')).toBeNull();
    await page.getByTestId('auto').click();
    expect(await geometry()).toEqual({ attribute: '50vh', height: 300 });
    expect(await envGeometry()).toEqual([{ attribute: null, height: 400 }, { attribute: null, height: 400 }]);
    expect(await page.getByTestId('environment-gradient').getAttribute('cx')).toBeNull();
    expect(await page.getByTestId('use').getAttribute('height')).toBe('50vh');
    const comparisons = await page.evaluate(async () => {
      const { svgGeometry } = await import('/svg-transform.js');
      const { sig, W, commit, Sm, Hooks } = await import('/rt.js');
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('width', '200'); svg.setAttribute('height', '400');
      const element = document.createElementNS(svg.namespaceURI, 'foreignObject');
      const control = document.createElementNS(svg.namespaceURI, 'foreignObject');
      for (const e of [element, control]) { e.setAttribute('width', '100'); e.style.fontSize = '20px'; svg.append(e); }
      document.body.append(svg);
      const value = sig('10vh');
      const held = [];
      const previous = Hooks.style;
      Hooks.style = (e, name, text) => { held.push(text); return false; };
      try {
        svgGeometry(element, 'height', value, Sm);
        return ['calc(10vh + 10px)', 'min(10vh, 50px)', 'max(10vh, 50px)', 'clamp(10px, 10vh, 100px)',
          'env(safe-area-inset-top, 3px)', 'env(viewport-segment-height 1 0, 25%)', '-exact-hairline'].map(length => {
          commit(() => W(value, length), 'test');
          const css = length === '-exact-hairline' ? 'var(--exact-hairline, 1px)' : length;
          control.style.height = ''; control.style.height = css;
          const fallback = { length, attribute: element.getAttribute('height'), css: element.style.height,
            control: control.style.height, height: element.getBoundingClientRect().height,
            expected: control.getBoundingClientRect().height, motion: held.at(-1) };
          commit(() => W(value, '10vh'), 'test');
          return { ...fallback, restored: [element.getAttribute('height'), element.style.height] };
        });
      } finally { Hooks.style = previous; svg.remove(); }
    });
    for (const result of comparisons) {
      expect(result.attribute, result.length).toBeNull();
      expect(result.css, result.length).toBe(result.control);
      expect(result.height, result.length).toBeCloseTo(result.expected, 3);
      expect(result.motion, result.length).toBe(result.length === '-exact-hairline' ? 'var(--exact-hairline, 1px)' : result.length);
      expect(result.restored).toEqual(['10vh', '']);
    }
    await browser.close();
    if (existsSync(webkit.executablePath())) {
      browser = await suiteBrowser('webkit');
      const safari = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
      await safari.goto(server.url.href);
      await safari.getByTestId('hairline-pattern').waitFor({ state: 'attached' });
      for (const id of ['hairline-pattern', 'hairline-static']) {
        expect(await safari.getByTestId(id).getAttribute('height')).toBe('1');
      }
      expect(await safari.getByTestId('hairline').getAttribute('height')).toBeNull();
      expect(await height(safari.getByTestId('hairline'))).toBeCloseTo(0.5, 1);
      const pixels = decodePng(await safari.getByTestId('hairline-paint').screenshot());
      let red = 0;
      for (let i = 0; i < pixels.data.length; i += 4) {
        if (Math.abs(pixels.data[i] - 225) <= 8 && Math.abs(pixels.data[i + 1] - 29) <= 8 && Math.abs(pixels.data[i + 2] - 72) <= 8) red++;
      }
      expect(red / (pixels.width * pixels.height)).toBeGreaterThan(0.4);
      expect(red / (pixels.width * pixels.height)).toBeLessThan(0.6);
    }
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);

test('font-relative SVG attributes follow root and inherited fonts', async () => {
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/svg-rem-'));
  const sheet = readFileSync(resolve(root, 'host/web/index.html'), 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1];
  let server, browser;
  try {
    const compiled = spawnSync(resolve(root, `target/debug/exact-web-js${EXE}`), [
      'js', resolve(import.meta.dir, 'conformance/svg-rem.contract'), '-o', dir,
    ], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${sheet}</style><link rel="stylesheet" href="/app.css">
        <div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`,
      { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href);
    await page.getByTestId('svg-bound').waitFor({ state: 'attached' });
    const check = async (font, rootFont, wide = false, omitted = false) => {
      const values = await page.evaluate(() => [...document.querySelectorAll('[data-testid$="-literal"], [data-testid$="-bound"]')].map(e => {
        const names = e.localName === 'radialGradient' ? ['cx', 'cy', 'r'] : ['x', 'y', 'width', 'height'];
        return { id: e.dataset.testid, attributes: names.map(name => e.getAttribute(name)),
          lengths: names.map(name => e[name].baseVal.value) };
      }));
      expect(values.length).toBe(16);
      for (const { id, attributes, lengths } of values) {
        const literal = id.endsWith('-literal');
        const gradient = id.startsWith('radialGradient');
        const extent = wide ? 3 * rootFont : 2 * font;
        const offset = wide ? font : 0.5 * rootFont;
        if (!literal && omitted) {
          expect(attributes, id).toEqual(gradient ? [null, null, null] : [null, null, null, null]);
        } else {
          expect(attributes, id).toEqual(literal ? (gradient ? ['2em', '0.5rem', '1em'] : ['0.5rem', '1em', '2em', '3rem'])
            : gradient ? [wide ? '3rem' : '2em', wide ? '1em' : '0.5rem', wide ? '3rem' : '2em']
              : [wide ? '1em' : '0.5rem', wide ? '1em' : '0.5rem', wide ? '3rem' : '2em', wide ? '3rem' : '2em']);
          expect(lengths, id).toEqual(literal ? (gradient ? [2 * font, 0.5 * rootFont, font] : [0.5 * rootFont, font, 2 * font, 3 * rootFont])
            : gradient ? [extent, offset, extent] : [offset, offset, extent, extent]);
        }
      }
      expect(errors).toEqual([]);
    };
    await check(20, 16);
    await page.evaluate(() => document.documentElement.style.fontSize = '24px');
    await check(20, 24);
    await page.getByTestId('resize').click();
    await check(30, 24);
    await page.getByTestId('change').click();
    await check(30, 24, true);
    await page.getByTestId('omit').click();
    await check(30, 24, true, true);
    await page.getByTestId('resize').click();
    await page.evaluate(() => document.documentElement.style.fontSize = '18px');
    await check(20, 18, true, true);
    await page.getByTestId('omit').click();
    await check(20, 18, true);
    await page.getByTestId('change').click();
    await check(20, 18);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);
