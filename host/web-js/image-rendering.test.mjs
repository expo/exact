import { expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { engineTests } from '../../scripts/coverage.mjs';
import { encodePng, decodePng } from '../../scripts/png.mjs';
import { withLook } from '../web/look.mjs';
import { glueDeclarations, glueNode, symbolSheet } from '../web/tests/glue-fixture.mjs';

const root = resolve(import.meta.dir, '../..');
const { chromium: test } = await engineTests(['chromium']);
const sprite = encodePng({ width: 3, height: 2, data: Uint8Array.from([
  255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
  0, 0, 0, 255, 255, 255, 255, 128, 255, 255, 255, 255,
]) });

test('template icons follow their controls in forced colours in both web renderers', async () => {
  const dir = mkdtempSync(resolve(root, 'target/forced-symbols-'));
  let browser, server;
  try {
    const template = 'symbol:icon||0 0 24 24|evenodd:M4 4H20V20H4Z M9 9H15V15H9Z|';
    const svg = id => `<svg width="24" height="24" viewBox="0 0 24 24" data-testid="ref-${id}"><path d="M4 4H20V20H4Z M9 9H15V15H9Z" fill="currentColor" fill-rule="evenodd"/></svg>`;
    const icon = id => `image "${template}" testId="${id}"`;
    const tab = selected => `button type="button" data-native data-button-style="plain" role="tab" aria-controls="panel" aria-selected="${selected}"`;
    const authored = '#008000';
    const accent = 'background-color:var(--exact-accent, AccentColor)';
    const raster = new Uint8Array(24 * 24 * 4);
    for (let y = 4; y < 20; y++) for (let x = 4; x < 20; x++) {
      if (x < 9 || x >= 15 || y < 9 || y >= 15) raster.set([208, 32, 128, 255], (y * 24 + x) * 4);
    }
    mkdirSync(resolve(dir, 'assets'));
    writeFileSync(resolve(dir, 'assets/mark.png'), encodePng({ width: 24, height: 24, data: raster }));
    writeFileSync(resolve(dir, 'assets/color.svg'), '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect x="4" y="4" width="16" height="16" fill="#d02080"/></svg>');
    writeFileSync(resolve(dir, 'app.contract'), `component App
  view
    row
      ${icon('plain')}
      link href="/unvisited"
        ${icon('unvisited')}
      link href="/visited"
        ${icon('visited')}
      button appearance="auto" disabled=true aria-label="Disabled"
        ${icon('disabled')}
      row role="tablist"
        button appearance="auto" buttonStyle="plain" role="tab" aria-selected=true aria-label="Selected"
          ${icon('selected')}
        button appearance="auto" buttonStyle="plain" role="tab" aria-selected=true disabled=true aria-label="Disabled selected tab"
          ${icon('disabled-selected')}
      ${icon('authored')} color="${authored}"
      link href="/unvisited"
        image "symbol:icon||0 0 24 24||color.svg" testId="full-colour"
      button appearance="none" background-color="AccentColor" aria-label="Accent"
        ${icon('accent')} color="light-dark(#ffffff, #000000)"
      button appearance="auto" buttonStyle="filled" aria-label="Tinted"
        ${icon('tinted')} -exact-tint-color="${authored}"
      image "assets/mark.png" testId="full-raster"
      row role="tablist" testId="sidebar"
        button appearance="auto" buttonStyle="plain" role="tab" aria-controls="panel" aria-selected=true aria-label="Selected sidebar tab"
          ${icon('sidebar-selected')}
        button appearance="auto" buttonStyle="plain" role="tab" aria-controls="panel" aria-selected=false aria-label="Sidebar tab"
          ${icon('sidebar-unselected')}
      button appearance="auto" buttonStyle="filled"
        ${icon('labelled')}
        text "Labelled" color="light-dark(#ffffff, #000000)"
`);
    const built = spawnSync(resolve(root, 'target/debug/exact-web-js'), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    // The lane builds Caltrain's render entry. It uses the wasm host's DOM projection.
    const target = process.env.CARGO_TARGET_DIR ?? resolve(root, 'target');
    const renderBin = process.platform === 'win32' ? 'caltrain-render.exe' : 'caltrain-render';
    const rendered = spawnSync(resolve(target, 'host-dev', renderBin), ['--plan', resolve(dir, 'app.plan'), '/'], { encoding: 'utf8' });
    expect(rendered.status, rendered.stderr || String(rendered.error)).toBe(0);
    const wasm = JSON.parse(rendered.stdout);
    expect(wasm.error).toBeUndefined();
    const wasmGlue = readFileSync(resolve(root, 'host/web/glue.js'), 'utf8');
    const wasmStyle = symbolSheet(wasmGlue);
    const wasmPaletteListener = glueNode(wasmGlue, node => node.type === 'ExpressionStatement'
      && node.expression.callee?.object?.arguments?.[0]?.value === '(forced-colors: active)' && node);
    const jsStyle = symbolSheet(readFileSync(resolve(root, 'host/web-js/symbols.js'), 'utf8'));
    const wasmPainter = glueDeclarations(readFileSync(resolve(root, 'host/web/wasm/views.js'), 'utf8'), ['refreshSymbols']);
    const shell = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8'));
    const style = shell.match(/<style>([\s\S]*?)<\/style>/)[1];
    // Chromium's emulated palettes give VisitedText and LinkText the same value.
    // Give :visited a distinct system colour so a mask stuck on LinkText fails.
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (path === 'js' || path === 'wasm') return new Response(`<!doctype html><style>${style}
        body{margin:0;background:Canvas}
        :is(img,svg)[data-testid]{display:block!important;width:24px!important;height:24px!important}
        #fixture #exact-root a:visited{color:GrayText!important}
        #fixture button > svg{color:inherit;background:transparent;margin:0;padding:0}</style>
        ${path === 'js' ? '<link rel="stylesheet" href="/app.css">' : `<style>${wasmStyle}</style>`}
        <div id="fixture" style="width:max-content"><div id="exact-root">${path === 'wasm' ? wasm.root : ''}</div></div>
        <template id="reference"><div style="display:flex">
          ${svg('plain')}
          <a data-view href="/unvisited">${svg('unvisited')}</a>
          <a data-view href="/visited">${svg('visited')}</a>
          <button type="button" data-native data-button-style="bordered" disabled>${svg('disabled')}</button>
          <div role="tablist">
            <${tab(true)}>${svg('selected')}</button>
            <${tab(true)} disabled>${svg('disabled-selected')}</button>
          </div>
          ${svg('authored')}
          <a data-view href="/unvisited"><img src="/assets/color.svg" data-testid="ref-full-colour" alt=""></a>
          <button type="button" style="${accent}">${svg('accent')}</button>
          <button type="button" data-native data-button-style="filled">${svg('tinted')}</button>
          <img src="/assets/mark.png" data-testid="ref-full-raster" alt="">
          <div role="tablist" data-exact-sidebar-tabs>
            <${tab(true)}>${svg('sidebar-selected')}</button>
            <${tab(false)}>${svg('sidebar-unselected')}</button>
          </div>
          <button type="button" data-native data-button-style="filled">${svg('labelled')}</button>
        </div></template>
        <script type="module">
          const root = document.getElementById('exact-root');
          ${path === 'js' ? 'const {default: app} = await import("/app.js"); app();' : `
            const views = new Map([...root.querySelectorAll('img')].entries()), host = {}, log = () => {};
            ${wasmPainter}
            ${wasmPaletteListener}
            addEventListener('resize', refreshSymbols);
            refreshSymbols();
          `}
          root.querySelector('[data-testid="sidebar"]').setAttribute('data-exact-sidebar-tabs', '');
          root.append(document.getElementById('reference').content);
          window.ready = true;
        </script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Missing', { status: 404 });
    } });
    browser = await suiteBrowser('chromium');
    const cases = ['plain', 'unvisited', 'visited', 'disabled', 'selected', 'disabled-selected', 'authored', 'full-colour',
      'accent', 'tinted', 'full-raster', 'sidebar-selected', 'sidebar-unselected', 'labelled'];
    const mismatches = [];
    for (const renderer of ['js', 'wasm']) for (const colorScheme of ['light', 'dark']) {
      const page = await browser.newPage({ colorScheme, contrast: 'no-preference' });
      try {
        await page.goto(new URL(renderer, server.url).href);
        await page.waitForFunction(() => window.ready && [...document.images].every(i => i.complete && i.naturalWidth)
          && document.querySelector('[data-testid="labelled"]').style.color);
        const labelled = page.getByTestId('labelled');
        const normalColor = await labelled.evaluate(el => el.style.color);
        await page.emulateMedia({ forcedColors: 'active' });
        const cdp = await page.context().newCDPSession(page);
        await cdp.send('DOM.enable');
        await cdp.send('CSS.enable');
        const { root: { nodeId } } = await cdp.send('DOM.getDocument');
        const { nodeIds } = await cdp.send('DOM.querySelectorAll', { nodeId, selector: 'a[href="/visited"]' });
        expect(nodeIds).toHaveLength(2);
        for (const nodeId of nodeIds) await cdp.send('CSS.forcePseudoState', { nodeId, forcedPseudoClasses: ['visited'] });
        if (renderer === 'js') expect(await page.locator('head style').allTextContents()).toContain(jsStyle);
        expect(await page.evaluate(() => matchMedia('(forced-colors: active)').matches)).toBe(true);
        const fixture = page.locator('#fixture');
        const boxes = await fixture.locator(':is(img,svg)[data-testid]').evaluateAll(elements => Object.fromEntries(elements.map(el => {
          const { x, y } = el.getBoundingClientRect();
          return [el.dataset.testid, [Math.ceil(x), Math.ceil(y)]];
        })));
        const { width, data } = decodePng(await fixture.screenshot());
        const pixel = (id, offset) => {
          const [x, y] = boxes[id];
          const at = ((y + offset) * width + x + offset) * 4;
          return [...data.subarray(at, at + 4)].join(',');
        };
        const canvas = colorScheme === 'light' ? '255,255,255,255' : '0,0,0,255';
        expect(pixel('ref-plain', 1)).toBe(canvas);
        expect(pixel('ref-visited', 6)).not.toBe(pixel('ref-unvisited', 6));
        expect(pixel('ref-disabled', 6)).not.toBe(pixel('ref-plain', 6));
        // A disabled selected tab keeps SelectedItemText over SelectedItem: GrayText there is unreadable in the dark palette.
        expect(pixel('ref-disabled-selected', 6)).toBe(pixel('ref-selected', 6));
        if (colorScheme === 'dark') expect(pixel('ref-selected', 6)).not.toBe(pixel('ref-plain', 6));
        expect(pixel('ref-full-colour', 6)).toBe('208,32,128,255');
        expect(pixel('ref-full-raster', 6)).toBe('208,32,128,255');
        for (const id of cases) {
          expect(pixel(`ref-${id}`, 6), `${renderer} ${colorScheme} ${id} reference`).not.toBe(pixel(`ref-${id}`, 1));
          for (const offset of id === 'full-colour' ? [6, 1] : [6, 1, 12]) {
            const actual = pixel(id, offset), expected = pixel(`ref-${id}`, offset);
            if (actual !== expected) mismatches.push({ renderer, colorScheme, id, offset, actual, expected });
          }
        }
        // A refresh during forced colours can stamp the forced RGB onto the
        // icon. When forced colours ends, the runtime reads its ordinary title.
        await page.evaluate(() => dispatchEvent(new Event('resize')));
        await page.waitForFunction(color => document.querySelector('[data-testid="labelled"]').style.color !== color, normalColor);
        await page.evaluate(() => {
          window.paletteChanged = new Promise(resolve => matchMedia('(forced-colors: active)').addEventListener('change', resolve, { once: true }));
        });
        await page.emulateMedia({ forcedColors: 'none' });
        await page.evaluate(async () => { await window.paletteChanged; });
        const restored = await labelled.evaluate(el => el.style.color);
        if (restored !== normalColor) mismatches.push({ renderer, colorScheme, id: 'labelled after forced colours', actual: restored, expected: normalColor });
      } finally { await page.close(); }
    }
    expect(mismatches).toEqual([]);
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 10000);

test('image rendering inherits and changes live, matching browser sprites at integer and fractional scales', async () => {
  const dir = mkdtempSync(resolve(root, 'target/image-rendering-web-'));
  let browser, server;
  try {
    mkdirSync(resolve(dir, 'assets'));
    writeFileSync(resolve(dir, 'assets/sprite.png'), sprite);
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state mode = "auto"
  action change
    mode = (mode == "auto" ? "crisp-edges" : mode == "crisp-edges" ? "pixelated" : "auto")
  view
    column image-rendering=mode
      button "Change" press=change testId="change"
      image "assets/sprite.png" width=6 height=4 testId="integer"
      image "assets/sprite.png" width=7 height=5 testId="fractional"
      image "assets/sprite.png" width=8 height=3 testId="stretched"
      image "assets/sprite.png" width=7 height=5 image-rendering="auto" testId="own"
`);
    const built = spawnSync(resolve(root, 'target/debug/exact-web-js'), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    const shell = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8'));
    const style = shell.match(/<style>([\s\S]*?)<\/style>/)[1];
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${style} body{background:white}</style><link rel="stylesheet" href="/app.css">
        <div id="exact-root"></div><img id="oracle" src="assets/sprite.png" style="display:block;object-fit:fill">
        <script type="module">import app from '/app.js';app();window.ready=true;</script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Missing', { status: 404 });
    } });
    browser = await suiteBrowser();
    for (const dpr of [1, 2]) {
      const page = await browser.newPage({ deviceScaleFactor: dpr });
      try {
        await page.goto(server.url.href);
        await page.waitForFunction(() => window.ready && [...document.images].every(i => i.complete && i.naturalWidth));
        const modes = ['auto', 'crisp-edges', 'pixelated', 'auto'];
        for (const mode of modes) {
          await page.waitForFunction(mode => getComputedStyle(document.querySelector('[data-testid="fractional"]')).imageRendering === mode, mode);
          for (const [id, width, height] of [['integer', 6, 4], ['fractional', 7, 5], ['stretched', 8, 3], ['own', 7, 5]]) {
            const expectedMode = id === 'own' ? 'auto' : mode;
            await page.evaluate(({ width, height, mode }) => Object.assign(document.getElementById('oracle').style, { width: `${width}px`, height: `${height}px`, imageRendering: mode }), { width, height, mode: expectedMode });
            const actual = decodePng(await page.locator(`[data-testid="${id}"]`).screenshot());
            const expected = decodePng(await page.locator('#oracle').screenshot());
            expect([...actual.data], `${id} ${mode} @${dpr}`).toEqual([...expected.data]);
          }
          await page.locator('[data-testid="change"]').click();
        }
      } finally { await page.close(); }
    }
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test('tinted rasters fit and position like images through static and bound sources and style updates', async () => {
  const dir = mkdtempSync(resolve(root, 'target/image-position-web-'));
  let browser, server;
  try {
    mkdirSync(resolve(dir, 'assets'));
    const data = new Uint8Array(40 * 40 * 4);
    for (let i = 0; i < data.length; i += 4) { data[i] = 255; data[i + 3] = 255; }
    writeFileSync(resolve(dir, 'assets/bar.png'), encodePng({ width: 40, height: 10, data: data.subarray(0, 40 * 10 * 4) }));
    writeFileSync(resolve(dir, 'assets/square.png'), encodePng({ width: 40, height: 40, data }));
    const squareSymbol = 'symbol:icon||0 0 24 24|M0 0H24V24H0Z|';
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state changed = false
  state invalid = "var(--missing) 75%"
  state invalidFallback = "var(--missing, bogus) 75%"
  state region = "env(-exact-layout-margins-inset-left, 90px) top"
  state mixedCaseFit = "CoNtAiN"
  state fallbackFit = "var(--missing, none)"
  action change
    changed = not changed
  view
    column
      button "Change" press=change testId="change"
      image "assets/bar.png" width=160 height=80 object-fit="none" object-position=invalid -exact-tint-color="#ff0000" testId="invalid"
      image "assets/bar.png" width=160 height=80 object-fit="none" object-position=(changed ? "7px 11px" : invalidFallback) -exact-tint-color="#ff0000" testId="invalid-fallback"
      image "assets/bar.png" width=160 height=80 object-fit="none" object-position="env(-exact-layout-margins-inset-left, 90px) top" -exact-tint-color="#ff0000" testId="static"
      image "assets/bar.png" width=160 height=80 object-fit="none" object-position="env(-exact-layout-margins-inset-left, 90px) top" testId="untinted-region"
      image "assets/bar.png" width=160 height=80 object-fit="none" object-position=region -exact-tint-color="#ff0000" testId="bound-region"
      image (changed ? "assets/bar.png#changed" : "assets/bar.png") width=160 height=80 object-fit=(changed ? mixedCaseFit : "none") object-position=(changed ? "right bottom" : "7px 11px") -exact-tint-color="#ff0000" testId="bound"
      image "assets/bar.png" width=160 height=80 object-fit=(changed ? fallbackFit : "scale-down") object-position=(changed ? "1em 0" : "7px 11px") -exact-tint-color="#ff0000" testId="style" font-size=20
      row width=70 align-items="center"
        image "${squareSymbol}" font-size=(changed ? 40 : 20) flex-shrink=0 testId="sizing-symbol"
        image "assets/square.png" flex=1 min-width=0 height=80 object-fit="scale-down" -exact-tint-color="#ff0000" testId="flex-raster"
      column object-fit="contain" object-position=(changed ? "left top" : "37px 23px") testId="parent"
        image (changed ? "assets/bar.png#changed" : "assets/bar.png") width=160 height=80 -exact-tint-color="#ff0000" testId="bound-default"
        image "assets/bar.png" width=160 height=80 object-fit="none" -exact-tint-color="#ff0000" testId="static-default-position"
        image "assets/bar.png" width=160 height=80 object-fit="none" testId="untinted-default"
        image "${squareSymbol}" width=160 height=80 object-fit="none" testId="symbol-default"
        input placeholder="Name" testId="placeholder"
`);
    const built = spawnSync(resolve(root, 'target/debug/exact-web-js'), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    const shell = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8'));
    const style = shell.match(/<style>([\s\S]*?)<\/style>/)[1];
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${style} body{background:white} img{background-color:white}</style><link rel="stylesheet" href="/app.css">
        <div id="exact-root"></div><img id="oracle" src="assets/bar.png" style="display:block;width:160px;height:80px;font-size:20px">
        <script type="module">import app from '/app.js';app();window.ready=true;</script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Missing', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage({ deviceScaleFactor: 1, viewport: { width: 840, height: 1400 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready && [...document.images].every(i => i.complete && i.naturalWidth));
    await page.locator('[data-testid="parent"]').evaluate(el => el.style.setProperty('--exact-placeholder-color', 'red'));
    const flexSizes = [];
    for (const changed of [false, true, false]) {
      const position = await page.locator('[data-testid="untinted-default"]').evaluate(el => getComputedStyle(el).objectPosition);
      expect(position).toBe('50% 50%');
      const symbolPosition = await page.locator('[data-testid="symbol-default"]').evaluate(el => getComputedStyle(el).maskPosition);
      expect(symbolPosition).toBe('50% 50%');
      await page.waitForFunction(size => {
        const symbol = document.querySelector('[data-testid="sizing-symbol"]');
        return symbol.complete && symbol.naturalWidth === size;
      }, changed ? 40 : 20);
      const raster = decodePng(await page.locator('[data-testid="flex-raster"]').screenshot());
      const redAt = (x, y) => {
        const pixel = (y * raster.width + x) * 4;
        return raster.data[pixel] === 255 && raster.data[pixel + 1] === 0 && raster.data[pixel + 2] === 0;
      };
      let inkWidth = 0, inkHeight = 0;
      for (let x = 0; x < raster.width; x++) if (redAt(x, Math.floor(raster.height / 2))) inkWidth++;
      for (let y = 0; y < raster.height; y++) if (redAt(Math.floor(raster.width / 2), y)) inkHeight++;
      flexSizes.push([raster.width, inkWidth, inkHeight]);
      const cases = [
        ['bound-default', 'fill', '50% 50%'],
        ['static-default-position', 'none', '50% 50%'],
        ['untinted-default', 'none', '50% 50%'],
        ['untinted-region', 'none', 'env(safe-area-inset-left, 90px) top'],
        ['bound-region', 'none', 'env(safe-area-inset-left, 90px) top'],
        ['invalid', 'none', 'var(--missing) 75%'],
        ['invalid-fallback', 'none', changed ? '7px 11px' : 'var(--missing, bogus) 75%'],
        ['static', 'none', 'env(safe-area-inset-left, 90px) top'],
        ['bound', changed ? 'contain' : 'none', changed ? 'right bottom' : '7px 11px'],
        ['style', changed ? 'none' : 'scale-down', changed ? '1em 0' : '7px 11px'],
      ];
      for (const [id, fit, position] of cases) {
        await page.evaluate(([fit, position]) => Object.assign(document.getElementById('oracle').style, { objectFit: fit, objectPosition: position }), [fit, position]);
        const actual = decodePng(await page.locator(`[data-testid="${id}"]`).screenshot()), expected = decodePng(await page.locator('#oracle').screenshot());
        expect(Buffer.from(actual.data).equals(Buffer.from(expected.data)), `${id}, changed=${changed}`).toBe(true);
      }
      if (flexSizes.length < 3) {
        await page.locator('[data-testid="sizing-symbol"]').evaluate(el => {
          window.symbolLoad = new Promise(resolve => el.addEventListener('load', () => resolve(), { once: true }));
        });
        await page.locator('[data-testid="change"]').click();
        await page.evaluate(async () => {
          await window.symbolLoad;
          await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        });
        await page.waitForFunction(() => [...document.images].every(i => i.complete && i.naturalWidth));
      }
    }
    expect(flexSizes).toEqual([[50, 40, 40], [30, 30, 30], [50, 40, 40]]);
    expect(await page.locator('[data-testid="placeholder"]').evaluate(el => {
      const oracle = document.createElement('input'); oracle.placeholder = 'Name';
      document.getElementById('exact-root').append(oracle);
      const same = getComputedStyle(el, '::placeholder').color === getComputedStyle(oracle, '::placeholder').color;
      oracle.remove(); return same;
    })).toBe(true);
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);


test('videos share the UA fit default and authored clears through static and bound rows', async () => {
  const dir = mkdtempSync(resolve(root, 'target/video-fit-web-'));
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state fit = "cover"
  action change
    fit = (fit == "cover" ? "unset" : fit == "unset" ? "" : "cover")
  view
    column
      button "Change" press=change testId="change"
      video preload="none" width=160 height=80 testId="absent"
      video preload="none" width=160 height=80 object-fit="unset" testId="unset"
      video preload="none" width=160 height=80 object-fit="" testId="empty"
      video preload="none" width=160 height=80 object-fit=fit testId="bound"
`);
    const built = spawnSync(resolve(root, 'target/debug/exact-web-js'), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<link rel="stylesheet" href="/app.css"><div id="exact-root"></div>
        <video id="oracle"></video><script type="module">import app from '/app.js';app();window.ready=true;</script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Missing', { status: 404 });
    } });
    browser = await suiteBrowser('chromium');
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    for (const fit of ['cover', 'unset', '', 'cover']) {
      await page.waitForFunction(fit => getComputedStyle(document.querySelector('[data-testid="bound"]')).objectFit === fit, fit === 'cover' ? 'cover' : 'fill');
      const result = await page.evaluate(fit => {
        const oracle = document.getElementById('oracle');
        const reference = value => { oracle.style.objectFit = value; return getComputedStyle(oracle).objectFit; };
        return ['absent', 'unset', 'empty', 'bound'].map((id, i) => [
          getComputedStyle(document.querySelector(`[data-testid="${id}"]`)).objectFit,
          reference(['', 'unset', 'unset', fit || 'unset'][i]),
        ]);
      }, fit);
      expect(result.every(([actual, expected]) => actual === expected), JSON.stringify(result)).toBe(true);
      await page.locator('[data-testid="change"]').click();
    }
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);
