// @exact/clipboard on the web with no agent (LLP 1133.006 §9 stage 1 items 4
// and 7): the fixture app built for the web and driven in Chrome, the oracle,
// whose clipboard the suite reads back. Each image lands as the pixels it
// should (the red PNG red, the APNG's frame 0 green), text as written, and in
// the A/B race B is what the clipboard holds.
//
// usage: bun packages/clipboard/test/web.mjs
import { join, extname, resolve } from 'node:path';
import { KINDS, checker, discard, makeFixture, root, run } from './fixture.mjs';

const { chromium } = await import(resolve(root, 'node_modules/playwright-core/index.mjs'));
const { app } = makeFixture();
run('bun', ['exact.mjs', 'web-build', '--render', 'none'], app);

// Where the build writes: the app's target, or a CARGO_TARGET_DIR the shell names.
const dist = join(process.env.CARGO_TARGET_DIR ? resolve(app, process.env.CARGO_TARGET_DIR) : join(app, 'target'), 'web-dist');
const types = { '.js': 'text/javascript', '.html': 'text/html', '.wasm': 'application/wasm', '.json': 'application/json', '.css': 'text/css' };
const server = Bun.serve({ port: 0, async fetch(req) {
  let path = new URL(req.url).pathname;
  if (path === '/') path = '/index.html';
  const file = Bun.file(join(dist, path));
  if (!(await file.exists())) return new Response('not found', { status: 404 });
  return new Response(file, { headers: { 'content-type': types[extname(path)] ?? 'application/octet-stream' } });
} });
const { check, failed } = checker();
const chrome = process.env.EXACT_CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({ executablePath: chrome, headless: true });
try {
  const context = await browser.newContext({ permissions: ['clipboard-read', 'clipboard-write'] });
  const page = await context.newPage();
  await page.goto(`http://localhost:${server.port}/`);
  const text = id => page.getByTestId(id).innerText({ timeout: 10000 }).catch(e => `(${e.message.split('\n')[0]})`);
  /** What the clipboard holds: its types, its text, and the image's size and top-left pixel. */
  const held = () => page.evaluate(async () => {
    const [item] = await navigator.clipboard.read();
    const out = { types: [...(item?.types ?? [])] };
    if (out.types.includes('text/plain')) out.text = await (await item.getType('text/plain')).text();
    if (out.types.includes('image/png')) {
      const bitmap = await createImageBitmap(await item.getType('image/png'));
      const c = new OffscreenCanvas(1, 1).getContext('2d');
      c.drawImage(bitmap, 0, 0, 1, 1);
      out.image = { width: bitmap.width, height: bitmap.height, pixel: [...c.getImageData(0, 0, 1, 1).data].slice(0, 3).join(',') };
    }
    return out;
  });
  const mounted = await text('mount');
  check('chrome', 'mount cannot read', mounted === 'NotAllowedError: clipboard.readText: NotAllowedError: person-required', mounted);
  const supports = await text('supports');
  check('chrome', 'ClipboardItem.supports answers from the declaration', supports === 'image/png=true text/plain=true text/html=true image/svg+xml=false', supports);
  const pixel = { png: '255,0,0', 'png-bytes': '255,0,0', both: '255,0,0', apng: '0,255,0' };
  for (const [kind, answer, landed] of KINDS) {
    const before = await text('wrote');
    await page.evaluate(() => navigator.clipboard.writeText('before'));
    await page.getByTestId(kind).click();
    const end = Date.now() + 10000;
    let got = before;
    while (Date.now() < end && (got = await text('wrote')) === before) await page.waitForTimeout(50);
    check('chrome', `${kind}: answers ${answer}`, got === answer, got);
    if (kind === 'html' || kind === 'html-only') {
      const stored = await page.evaluate(async () => {
        const [item] = await navigator.clipboard.read({ unsanitized: ['text/html'] });
        return (await item.getType('text/html')).text();
      });
      if (kind === 'html-only') {
        check('chrome', 'a hidden document body is absent from the clipboard', !stored.includes('private draft'), stored);
      } else {
        check('chrome', 'the unsanitized read already contains clean HTML', stored.includes('<b>Résumé</b>') &&
          !/script|onclick|discard/i.test(stored), stored);
        await page.evaluate(() => {
          const editor = document.body.appendChild(document.createElement('div'));
          editor.contentEditable = 'true';
          editor.id = 'clipboard-paste-target';
          editor.focus();
        });
        const editor = page.locator('#clipboard-paste-target');
        await editor.press(process.platform === 'darwin' ? 'Meta+v' : 'Control+v');
        const pasted = await editor.evaluate(el => ({
          text: el.textContent.trim(),
          bold: [...el.querySelectorAll('*')].some(e => e.textContent === 'Résumé' && Number(getComputedStyle(e).fontWeight) >= 600),
        }));
        check('chrome', 'a rich editor pastes the formatted note', pasted.text === 'Résumé note' && pasted.bold, JSON.stringify(pasted));
        await editor.evaluate(el => el.remove());
      }
    }
    if (kind === 'both') {
      for (const [id, expected] of [['paste', 'https://example.com/red.png | png 137 80'], ['open-url', 'https://example.com/red.png'], ['read-pair', 'https://example.com/red.png | https://example.com/red.png']]) {
        await page.getByTestId(id).click();
        await page.waitForFunction(s => document.querySelector('[data-testid=wrote]')?.textContent === s, expected, { timeout: 5000 }).catch(() => {});
        const seen = await text('wrote');
        check('chrome', id, seen === expected, seen);
      }
    }
    const h = await held();
    if (!landed) { check('chrome', `${kind}: leaves the clipboard as it was`, h.text === 'before', JSON.stringify(h)); continue; }
    const ok = landed.types.every(t => h.types.includes(t)) && (landed.text === undefined || h.text === landed.text)
      && (!landed.image || (h.image?.width === 8 && h.image?.height === 8 && h.image.pixel === pixel[kind]));
    check('chrome', `${kind}: the clipboard holds ${JSON.stringify(landed)}${pixel[kind] ? ` in ${pixel[kind]}` : ''}`, ok, JSON.stringify(h));
  }
  await page.evaluate(() => navigator.clipboard.writeText('Read from Return'));
  await page.getByTestId('paste-key').press('Enter');
  await page.waitForFunction(() => document.querySelector('[data-testid=wrote]')?.textContent === 'Read from Return', null, { timeout: 5000 }).catch(() => {});
  const submitted = await text('wrote');
  check('chrome', 'Return submits a person-started read', submitted === 'Read from Return', submitted);
} finally {
  await browser.close();
  server.stop();
}
discard(app);
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
