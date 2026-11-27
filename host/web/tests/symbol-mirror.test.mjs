import { symbolSheet, hostFixtureFile as readFileSync, glueDeclarations } from './glue-fixture.mjs';
import { expect } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests();
const ROOT = resolve(import.meta.dir, '../../..');
const schema = JSON.parse(readFileSync(resolve(ROOT, 'kernel/tables/schema.json'), 'utf8'));
const roles = Object.fromEntries(schema.symbols.map(([role, , d, mirror]) => [role, [d, !!mirror]]));

const RUNTIMES = {
  'host/web/glue.js': [
    'const views = new Map([...document.querySelectorAll("#exact-root img")].entries()), log = () => {}, host = {};',
    ['tintFit', 'refreshSymbols'],
    'refreshSymbols();',
  ],
  'host/web-js/symbols.js': [
    'const rolesReady = true, journal = [], clock = { now: 0 }, appSource = () => {}, Images = new Set(document.querySelectorAll("#exact-root img")), image = e => Images.add(e);',
    ['refresh'],
    'refresh();',
  ],
};

const sampleRed = async (page, id, x, y) => {
  const box = await page.locator(`#${id}`).boundingBox();
  const png = await page.screenshot({ clip: { x: box.x + x, y: box.y + y, width: 1, height: 1 } });
  return page.evaluate(async (data) => {
    const image = new Image();
    image.src = `data:image/png;base64,${data}`;
    await image.decode();
    const canvas = Object.assign(document.createElement('canvas'), { width: 1, height: 1 });
    const ctx = canvas.getContext('2d');
    ctx.drawImage(image, 0, 0);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return r > 200 && g < 80 && b < 80;
  }, png.toString('base64'));
};

for (const engine of ['chromium', 'firefox', 'webkit']) {
  for (const [file, [stand, names, run]] of Object.entries(RUNTIMES)) {
    engineTest[engine](`${engine}: directional symbols mirror in CSS rtl and update when direction changes (${file})`, async () => {
      const source = readFileSync(resolve(ROOT, file), 'utf8');
      const style = symbolSheet(source);
      const [backPath] = roles.back;
      const [playPath] = roles.play;
      const content = `<div id="wrap" style="display:flex;gap:16px;padding:16px;direction:ltr">
        <img id="back" data-symbol-source="symbol:back" data-symbol-path="${backPath}" data-symbol-mirror style="--exact-symbol-size-authored:1;font-size:48px;width:48px;height:48px">
        <img id="play" data-symbol-source="symbol:play" data-symbol-path="${playPath}" style="--exact-symbol-size-authored:1;font-size:48px;width:48px;height:48px">
        <img id="offset" data-symbol-source="symbol:back" data-symbol-path="${backPath}" data-symbol-mirror data-symbol-viewbox="10 0 24 24" style="--exact-symbol-size-authored:1;font-size:48px;width:48px;height:48px">
      </div>`;
      const html = withLook(readFileSync(resolve(ROOT, 'host/web/index.html'), 'utf8'))
        .replace('<div id="exact-root"></div>', `<style>${style} #exact-root img { --exact-tint: rgb(255, 0, 0) !important; }</style><div id="exact-root">${content}</div>`)
        .replace('<script type="module" src="./glue.js"></script>', `<script>${stand} ${glueDeclarations(source, names)} globalThis.paint = () => { ${run} };</script>`);
      const suite = await suiteBrowser(engine, { timeout: 10000 });
      try {
        const page = await suite.newPage({ viewport: { width: 240, height: 100 }, deviceScaleFactor: 1 });
        const errors = [];
        page.on('pageerror', (error) => errors.push(String(error)));
        await page.setContent(html);
        const repaint = async () => {
          await page.evaluate(() => globalThis.paint());
          await page.waitForFunction(() => [...document.images].every((e) => e.complete));
          await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
        };

        // LTR: `back` chevron tip is at x=8/24 (16px); x=16/24 (32px) is empty.
        await repaint();
        expect([await sampleRed(page, 'back', 16, 24), await sampleRed(page, 'back', 32, 24)]).toEqual([true, false]);
        expect(await page.evaluate(() => decodeURIComponent(document.getElementById('back').symbolMask))).not.toContain('scale(-1 1)');

        // Flip CSS direction (not HTML `dir`) to RTL without changing source or size.
        await page.evaluate(() => { document.getElementById('wrap').style.direction = 'rtl'; });
        await repaint();
        expect([await sampleRed(page, 'back', 16, 24), await sampleRed(page, 'back', 32, 24)]).toEqual([false, true]);
        expect(await page.evaluate(() => decodeURIComponent(document.getElementById('back').symbolMask))).toContain('translate(24 0) scale(-1 1)');
        expect(await page.evaluate(() => decodeURIComponent(document.getElementById('play').symbolMask))).not.toContain('scale(-1 1)');
        // Nonzero minX viewBox (`10 0 24 24`) pivots around 2*10 + 24 = 44.
        expect(await page.evaluate(() => decodeURIComponent(document.getElementById('offset').symbolMask))).toContain('translate(44 0) scale(-1 1)');

        // Flip back to LTR: cache key invalidates cleanly.
        await page.evaluate(() => { document.getElementById('wrap').style.direction = 'ltr'; });
        await repaint();
        expect([await sampleRed(page, 'back', 16, 24), await sampleRed(page, 'back', 32, 24)]).toEqual([true, false]);
        expect(errors).toEqual([]);
      } finally {
        await suite.close();
      }
    }, 20000);
  }
}

engineTest.chromium('symbols.js draw and unsymbol toggle and clear data-symbol-mirror cleanly', async () => {
  const source = readFileSync(resolve(ROOT, 'host/web-js/symbols.js'), 'utf8');
  const suite = await suiteBrowser('chromium', { timeout: 10000 });
  try {
    const page = await suite.newPage();
    await page.setContent(`<script>
      const Table = {
        back: ["M15 5 8 12l7 7", 2],
        "star-fill": ["M0 0h1Z", 1],
        send: ["M12 20V4", 0],
        logo: ["", 1, "0 0 24 24", "logo.svg"]
      };
      const image = () => {};
      ${glueDeclarations(source, ['iconForm', 'draw', 'unsymbol'])}
      globalThis.run = () => {
        const e = document.createElement('img');
        e.setAttribute('data-symbol-source', 'symbol:back');
        draw(e, 'back');
        const s1 = [e.hasAttribute('data-symbol-fill'), e.hasAttribute('data-symbol-mirror')];
        draw(e, 'star-fill');
        const s2 = [e.hasAttribute('data-symbol-fill'), e.hasAttribute('data-symbol-mirror')];
        draw(e, 'back');
        draw(e, 'logo');
        const s3 = [e.hasAttribute('data-symbol-fill'), e.hasAttribute('data-symbol-mirror'), e.hasAttribute('data-symbol-file')];
        draw(e, 'back');
        unsymbol(e);
        const s4 = [e.hasAttribute('data-symbol-fill'), e.hasAttribute('data-symbol-mirror'), e.hasAttribute('data-symbol-source')];
        return [s1, s2, s3, s4];
      };
    </script>`);
    expect(await page.evaluate(() => globalThis.run())).toEqual([
      [false, true],
      [true, false],
      [false, false, true],
      [false, false, false],
    ]);
  } finally {
    await suite.close();
  }
}, 10000);

for (const engine of ['chromium', 'firefox', 'webkit']) {
  engineTest[engine](`${engine}: projected header Back mirrors from CSS direction and updates across commits`, async () => {
    const html = withLook(readFileSync(resolve(ROOT, 'host/web/index.html'), 'utf8'))
      .replace('<div id="exact-root"></div>', () => `<div id="exact-root">
        <main navigationKey="2" navigationBack="back" style="display:block">
          <div navigationKey="1"><header><h1>Settings</h1></header></div>
          <div navigationKey="2" id="css-rtl"><header style="display:flex;align-items:center;direction:rtl"><button type="button" id="back" data-exact-on="press" aria-label="Back"><img data-symbol-source="symbol:back" data-symbol-path="${roles.back[0]}" data-symbol-mirror style="width:20px;height:20px"></button><h2 role="heading" data-exact-text>Details</h2></header></div>
        </main>
        <main navigationKey="2" navigationBack="back" dir="rtl" style="display:block">
          <div navigationKey="1"><header><h1>Settings</h1></header></div>
          <div navigationKey="2" id="dir-rtl-css-ltr"><header style="display:flex;align-items:center;direction:ltr"><button type="button" id="back" data-exact-on="press" aria-label="Back"><img data-symbol-source="symbol:back" data-symbol-path="${roles.back[0]}" data-symbol-mirror style="width:20px;height:20px"></button><h2 role="heading" data-exact-text>Details</h2></header></div>
        </main>
      </div>`)
      .replace('<script type="module" src="./glue.js"></script>', () => `<script type="module">${readFileSync(resolve(ROOT, 'host/web/navigation.js'), 'utf8')}
        const shell = document.getElementById('exact-root');
        navigation.connect(shell, () => false, () => {});
        navigation.project(shell, () => {});
        window.project = () => navigation.project(shell, () => {});
        window.ready = true;</script>`);
    const suite = await suiteBrowser(engine, { timeout: 10000 });
    try {
      const page = await suite.newPage({ viewport: { width: 400, height: 300 }, deviceScaleFactor: 1 });
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      await page.setContent(html);
      await page.waitForFunction(() => window.ready);

      const read = () => page.evaluate(() => ['css-rtl', 'dir-rtl-css-ltr'].map((id) => {
        const button = document.querySelector(`#${id} > header`).shadowRoot.querySelector('button[part="back"]');
        const svg = button.querySelector('svg');
        return [button.hasAttribute('data-rtl'), getComputedStyle(svg).scale];
      }));

      // CSS `direction: rtl` mirrors Back even without HTML `dir="rtl"`;
      // HTML `dir="rtl"` overridden by CSS `direction: ltr` does not mirror.
      expect(await read()).toEqual([
        [true, '-1 1'],
        [false, 'none'],
      ]);

      // Flipping CSS direction on a subsequent commit updates `data-rtl` cleanly.
      await page.evaluate(() => {
        document.querySelector('#css-rtl > header').style.direction = 'ltr';
        document.querySelector('#dir-rtl-css-ltr > header').style.direction = 'rtl';
        window.project();
      });
      expect(await read()).toEqual([
        [false, 'none'],
        [true, '-1 1'],
      ]);
      expect(errors).toEqual([]);
    } finally {
      await suite.close();
    }
  }, 20000);
}
