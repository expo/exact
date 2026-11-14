import { beforeAll, afterAll, test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

let dir, dist;
beforeAll(async () => {
  const root = resolve(import.meta.dir, '../..');
  dir = mkdtempSync(resolve(root, 'target/image-document-'));
  dist = resolve(dir, 'dist');
  writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build activate=idle

component Images
  view
    column testId="images"
      image "assets/hero.png" alt="Hero"
      image "assets/ordinary.png" alt="Ordinary"
      for name in ["a", "b"] key=name
        image \`\${name}.png\` alt=name
`);
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'caltrain', '--contract', resolve(dir, 'app.contract'), '--out', dist, '--render', 'rust'], {
    cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
  expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
}, 1_800_000);
afterAll(() => { if (dir) rmSync(dir, { recursive: true, force: true }); });

test('Rust-rendered images keep asynchronous decoding through adoption', async () => {
  const html = readFileSync(resolve(dist, 'index.html'), 'utf8');
  const images = html.match(/<img [^>]*>/g);
  expect(images).toHaveLength(4);
  for (const image of images) expect(image.match(/decoding="async"/g)).toHaveLength(1);
  expect(images.map(image => image.includes('loading="lazy"'))).toEqual([false, false, true, true]);
  expect(images.map(image => image.includes('fetchpriority="high"'))).toEqual([true, false, false, false]);
  expect(html).toContain('<link rel="preload" as="image" href="assets/hero.png">');
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAQAAAACCAYAAAB/qH1jAAAAEklEQVR4nGN44ODwHxkzoAsAAEE1EvnzbXFzAAAAAElFTkSuQmCC', 'base64');
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path.endsWith('.png')) return new Response(png, { headers: { 'content-type': 'image/png' } });
    return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.addInitScript(() => {
      const observer = new MutationObserver(() => {
        const images = [...document.querySelectorAll('[data-testid="images"] img')];
        if (images.length !== 4) return;
        globalThis.servedImages = images;
        globalThis.servedDecoding = images.map(image => image.getAttribute('decoding'));
        observer.disconnect();
      });
      observer.observe(document, { childList: true, subtree: true });
    });
    await page.goto(`${server.url.href}?agent=1`);
    await page.waitForFunction(() => globalThis.exact?.journal?.some(line => line.endsWith('adopted the document')));
    expect(await page.evaluate(() => ({
      before: servedDecoding,
      after: [...document.querySelectorAll('[data-testid="images"] img')].map(image => image.getAttribute('decoding')),
      kept: servedImages.every((image, index) => image === document.querySelectorAll('[data-testid="images"] img')[index]),
    }))).toEqual({ before: ['async', 'async', 'async', 'async'], after: ['async', 'async', 'async', 'async'], kept: true });
  } finally { await browser?.close(); server.stop(true); }
}, 60000);
