// LLP 1021 D3's checkmark on the web, through both runtimes in Chrome, Firefox and WebKit: in a menu whose rows can be
// checked, each checked row paints a mark before its face and an unchecked one paints none, whatever the face's
// layout: a title alone, a symbol before or after it, a written width, and a direction the author wrote, which the
// runtimes write as the face's grid template (button_css.rs, the JS target's style rows), changed while the page runs.
// A raster face, which its author lays out, takes the column after the padding its author wrote, bound or not, and
// not an ancestor's, and keeps its authored borders.
// host/web/tests/neutral-look.test.mjs holds the sheet's rules over hand-written rows (nesting, a List, forced colours).
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { open } from '../../../scripts/agent.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { decodePng } from '../../../scripts/png.mjs';
import { serveBuildTree } from '../serve.mjs';

const ROOT = resolve(import.meta.dir, '../../..');
const SOURCE = `routes nav
  home "/"

component App
  state stacked = false
  action flip
    stacked = not stacked
  view
    main navigationKey=\`\${top(nav).id}\` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` position="absolute" inset=0
          header display="flex" align-items="center" padding="8px 16px"
            text "Filter" role="heading" aria-level=1 flex=1
            button press=flip testId="flip"
              text "Flip"
            button popovertarget="menu" aria-label="Filter" testId="open"
              image "symbol:more"
            button popovertarget="menu2" aria-label="More" testId="open2"
              image "symbol:more"
          // A menu's ancestor pads itself, bound here and statically below; neither padding is a row's own.
          column padding-left=(stacked ? 32 : 16)
            column id="menu" popover="auto" role="menu"
              button role="menuitemradio" aria-checked=true testId="title"
                text "All"
              button role="menuitemradio" aria-checked=false testId="off"
                text "Active"
              button role="menuitemcheckbox" aria-checked=true testId="box"
                text "Archived"
              button role="menuitemradio" aria-checked=true flex-direction="row" testId="leading"
                image "symbol:star"
                text "Starred"
              button role="menuitemradio" aria-checked=true testId="trailing"
                text "Shared"
                image "symbol:star"
              button role="menuitemradio" aria-checked=true flex-direction=(stacked ? "column" : "row") testId="turning"
                image "symbol:star"
                text "Turning"
              button role="menuitemradio" aria-checked=true width="100%" testId="wide"
                text "Wide"
              button role="menuitemradio" aria-checked=true white-space="nowrap" testId="long"
                text "A title far wider than the menu that holds it, which ends in an ellipsis"
              button role="menuitemradio" aria-checked=true display="flex" align-items="center" gap=8 height=44 testId="raster"
                image "assets/maps.png" width=24 height=24
                text "Maps"
              button role="menuitemradio" aria-checked=false display="flex" align-items="center" gap=8 height=44 padding=4 border-bottom-width=2 border-bottom-style="solid" border-bottom-color="#ff9999" testId="raster-off"
                image "assets/maps.png" width=24 height=24
                text "Waze"
              button role="menuitemradio" aria-checked=true display="flex" align-items="center" gap=8 height=44 padding-left=(stacked ? 12 : 4) border-bottom-width=2 border-bottom-style="solid" border-bottom-color="#ff9999" testId="raster-bound"
                image "assets/maps.png" width=24 height=24
                text "Google"
              button role="menuitemradio" aria-checked=true direction="rtl" display="flex" align-items="center" gap=8 height=44 padding-left=4 padding-right=12 testId="raster-rtl"
                image "assets/maps.png" width=24 height=24
                text "Apple"
          // A row in a button is no native one, so this menu is apart: its one raster row is all it checks.
          button appearance="none" padding-left=32 testId="holder"
            column id="menu2" popover="auto" role="menu"
              button role="menuitemradio" aria-checked=true display="flex" align-items="center" gap=8 height=44 testId="nested"
                image "assets/maps.png" width=24 height=24
                text "Maps"
`;
const ROWS = ['title', 'off', 'box', 'leading', 'trailing', 'turning', 'wide', 'long', 'raster', 'raster-off', 'raster-bound', 'raster-rtl'];
const RASTERS = ['raster', 'raster-off', 'raster-bound', 'raster-rtl'];

// Each row's mark as painted: the page is shot with every face hidden, so the only ink in a row is its mark's (a
// pixel nearer the mark's colour than the menu's ground, the check's antialiased edges included). `painted`: some ink with ground among it, no larger than the em square (a check, not a block); `before`: the
// ink ends before the face's first box starts, in the row's direction; `centred`: its middle is within a pixel of the face's.
async function marks(s, png) {
  const rows = await s.carrier.evaluate(`(() => {
    const rgba = (c) => { const k = document.createElement('canvas').getContext('2d'); k.fillStyle = c; k.fillRect(0, 0, 1, 1); return [...k.getImageData(0, 0, 1, 1).data]; };
    return ${JSON.stringify(ROWS)}.map((id) => {
      const b = document.querySelector('[data-testid="' + id + '"]'), r = b.getBoundingClientRect(), boxes = [...b.children].map((c) => c.getBoundingClientRect()), rtl = getComputedStyle(b).direction === 'rtl', face = rtl ? Math.max(...boxes.map((c) => c.right)) : Math.min(...boxes.map((c) => c.x)), middle = (Math.min(...boxes.map((c) => c.top)) + Math.max(...boxes.map((c) => c.bottom))) / 2;
      const mark = getComputedStyle(b, '::after'), ink = rgba(mark.backgroundColor)[3] ? mark.backgroundColor : getComputedStyle(b).color;
      return { id, rtl, row: [r.x, r.y, r.width, r.height], face, middle, fs: parseFloat(getComputedStyle(b).fontSize), ink: rgba(ink), ground: rgba(getComputedStyle(b.closest('[popover]')).backgroundColor) };
    });
  })()`);
  await s.carrier.evaluate(`(() => { const h = document.createElement('style'); h.id = 'hide-faces'; h.textContent = '#menu button > * { visibility: hidden !important; }'; document.head.append(h); })()`);
  await s.screenshot(png);
  await s.carrier.evaluate(`document.getElementById('hide-faces').remove()`);
  const { width, data } = decodePng(readFileSync(png));
  const far = (c, i) => Math.abs(data[i] - c[0]) + Math.abs(data[i + 1] - c[1]) + Math.abs(data[i + 2] - c[2]);
  return Object.fromEntries(rows.map(({ id, rtl, row: [rx, ry, rw, rh], face, middle, fs, ink, ground }) => {
    let [x0, y0, x1, y1, inked, bare] = [Infinity, Infinity, -1, -1, 0, 0];
    for (let y = Math.ceil(ry); y < ry + rh - 1; y++) for (let x = Math.ceil(rx); x < rx + rw - 1; x++) if (far(ink, (y * width + x) * 4) < far(ground, (y * width + x) * 4)) { x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); }
    for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) { const i = (y * width + x) * 4; if (far(ink, i) < far(ground, i)) inked++; else if (far(ground, i) < 30) bare++; }
    const painted = inked > 4 && bare > inked / 4 && x1 - x0 < fs && y1 - y0 < fs;
    return [id, { painted, before: x1 < 0 || (rtl ? x0 >= face : x1 + 1 <= face), centred: x1 < 0 || Math.abs((y0 + y1 + 1) / 2 - middle) <= 1 }];
  }));
}

const expected = Object.fromEntries(ROWS.map((id) => [id, { painted: !['off', 'raster-off'].includes(id), before: true, centred: true }]));

// A raster row's face offset from its inline-start border edge in ems, after the padding its author wrote and the mark's column,
// and its authored bottom border as computed and as painted (a pixel in it, off the menu's surface).
async function rasters(s, png, ids = RASTERS) {
  const rows = await s.carrier.evaluate(`${JSON.stringify(ids)}.map((id) => {
    const b = document.querySelector('[data-testid="' + id + '"]'), cs = getComputedStyle(b), r = b.getBoundingClientRect();
    const boxes = [...b.children].map((c) => c.getBoundingClientRect()), rtl = cs.direction === 'rtl';
    const lead = rtl ? r.right - Math.max(...boxes.map((c) => c.right)) : Math.min(...boxes.map((c) => c.x)) - r.x;
    return { offset: Math.round(lead / parseFloat(cs.fontSize) * 1000) / 1000, border: [cs.borderBottomWidth, cs.borderBottomStyle, cs.borderBottomColor], at: [Math.round(r.x + r.width / 2), Math.ceil(r.bottom) - 1] };
  })`);
  await s.screenshot(png);
  const { width, data } = decodePng(readFileSync(png));
  return rows.map(({ offset, border, at: [x, y] }) => [offset, ...border, [...data.subarray((y * width + x) * 4, (y * width + x) * 4 + 3)].join()]);
}
const line = ['2px', 'solid', 'rgb(255, 153, 153)', '255,153,153'];
for (const [runtime, webDist] of Object.entries({ wasm: process.env.EXACT_WEB_DIST, JS: true })) {
  const check = need('fixture', webDist ? null : 'no wasm web dist: name one with EXACT_WEB_DIST');
  check(`${runtime}: a checkable menu's rows lead with their marks in every face layout and direction`, async () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-menu-checks-'));
    let server;
    try {
      const plan = resolve(dir, 'app.plan');
      writeFileSync(resolve(dir, 'app.contract'), SOURCE);
      const built = spawnSync('cargo', ['run', '-q', '-p', 'contract', '--', 'build', resolve(dir, 'app.contract'), '-o', plan], { cwd: ROOT, encoding: 'utf8' });
      expect(built.status, built.stderr).toBe(0);
      let options = { plan, webDist: runtime === 'wasm' ? resolve(webDist) : undefined };
      if (runtime === 'JS') {
        const out = resolve(dir, 'dist'), build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'caltrain', '--plan', plan, '--out', out, '--render', 'none'], { cwd: ROOT, encoding: 'utf8' });
        expect(build.status, build.stderr).toBe(0);
        server = createServer((req, res) => serveBuildTree(out, req, res));
        await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
        options = { url: `http://127.0.0.1:${server.address().port}/` };
      }
      for (const browser of ['chrome', 'firefox', 'webkit']) {
        const s = await open({ host: 'web', browser, size: '420x700', ...options });
        try {
          await s.tap('open');
          await s.clock('settle');
          expect([browser, 'row', await marks(s, resolve(dir, `${browser}-row.png`))]).toEqual([browser, 'row', expected]);
          expect([browser, 'rasters', (await rasters(s, resolve(dir, `${browser}-rasters.png`))).map((r, i) => [1, 2].includes(i) ? r : r.slice(0, 1))])
            .toEqual([browser, 'rasters', [[2.125], [1.75, ...line], [1.75, ...line], [2.25]]]);
          // A title kept to one line still ends in an ellipsis inside its row, after the mark's column.
          expect([browser, 'long', await s.carrier.evaluate(`(() => { const b = document.querySelector('[data-testid="long"]'), t = b.querySelector('[data-exact-text]'), r = b.getBoundingClientRect(), l = t.getBoundingClientRect();
            return [t.scrollWidth > t.clientWidth, getComputedStyle(t).textOverflow, l.right <= r.right - parseFloat(getComputedStyle(b).paddingRight)]; })()`)]).toEqual([browser, 'long', [true, 'ellipsis', true]]);
          // The direction changes while the page runs: the face stacks its symbol over its title, after the mark.
          await s.tap('flip');
          await s.clock('settle');
          await s.tap('open');
          await s.clock('settle');
          expect([browser, 'column', await s.carrier.evaluate(`(([image, title]) => title.y > image.bottom)([...document.querySelector('[data-testid="turning"]').children].map((c) => c.getBoundingClientRect()))`)]).toEqual([browser, 'column', true]);
          expect([browser, 'column', await marks(s, resolve(dir, `${browser}-column.png`))]).toEqual([browser, 'column', expected]);
          // A bound padding changes with it, and the column still follows it; an ancestor's change is not a row's.
          const after = await rasters(s, resolve(dir, `${browser}-bound.png`));
          expect([browser, 'bound', after[0][0], after[2]]).toEqual([browser, 'bound', 2.125, [2.25, ...line]]);
          await s.tap('open2');
          await s.clock('settle');
          expect([browser, 'nested', (await rasters(s, resolve(dir, `${browser}-nested.png`), ['nested']))[0][0]]).toEqual([browser, 'nested', 2.125]);
        } finally { await s.close(); }
      }
    } finally {
      server?.close();
      rmSync(dir, { recursive: true, force: true });
    }
  }, 600000);
}
