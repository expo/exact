// What WebKit paints while a layout transition moves a box: filmed there and read back in Chromium, since a WebKit
// screenshot is drawn on the page's thread and shows where the box is placed, not where it is painted.
import { afterAll, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { moduleRoute, serveModules } from './module-server.mjs';

const PRESENCE = resolve(import.meta.dirname, '../presence-glue.js');
const check = need('webkit and chromium', (await browserMissing('webkit')) || (await browserMissing('chromium')));
const dir = mkdtempSync(resolve(tmpdir(), 'presence-webkit-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

// Two boxes move 200 points right in 1.5 s: `exact` by the module's layout transition, paused a while on the way,
// and `control` by a bare translate added to its `translate` of `none`, which WebKit paints past the box's old place
const box = left => `left:${left}px;top:20px;background:#0f0;--exact-layout-transition:1500 0 linear`;
const page = `<!doctype html><style>body { margin:0; background:#000 } #root { position:relative; width:600px; height:140px }
  #root > div { position:absolute; width:40px; height:40px }</style>
<div id="root"><div id="exact" style="${box(100)}"></div><div id="control" style="left:100px;top:80px;background:#f0f"></div></div>
<script>globalThis.exact = {};</script>
<script type="module">
  await import('./presence-glue.js');
  const root = document.getElementById('root'), el = document.getElementById('exact'), control = document.getElementById('control');
  const live = exact.presence(root), views = new Map([['exact', el]]);
  const frame = () => new Promise(requestAnimationFrame);
  // Both moves, from the boxes at rest until they are at rest again, each for 60 painted frames, which the film's late
  // start and its 25 frames a second still catch. The module's move is paused before half way until the bare move,
  // which started with it and is as long, has finished, so past the end it had, and then goes on to its own end; the
  // pause seeks every animation the box has to where the move is.
  const frames = async n => { for (let i = 0; i < n; i++) await frame(); };
  const rest = () => frames(60);
  window.go = async () => {
    await rest();
    const batch = { ops: [{ op: 'style', id: 'exact', css: ${JSON.stringify(box(300))} }] };
    live.before(batch, views);
    el.style.cssText = batch.ops[0].css;
    live.after(batch, views);
    control.style.left = '300px';
    const move = el.getAnimations().find(a => a.id === 'exact-layout-move');
    const bare = control.animate([{ translate: '-200px 0px' }, { translate: '0px 0px' }], { duration: 1500, easing: 'linear', fill: 'backwards', composite: 'add' });
    while (move.currentTime < 300) await frame();
    // Every animation the box has is paused and sought where the move is, as a tool that seeks a page's animations does
    for (const a of el.getAnimations()) a.pause();
    const pausedAt = move.currentTime;
    for (const a of el.getAnimations()) a.currentTime = pausedAt;
    await bare.finished;
    await frames(10);
    move.play();
    const finished = await move.finished.then(() => true, () => false);
    await rest();
    return { pausedAt, finished };
  };
  window.ready = true;
</script>`;
// Plays the film and reads each frame's leftmost painted point of each box's row
const decode = `<!doctype html><video muted playsinline src="/film.webm"></video><canvas width="600" height="140"></canvas>`;

check('WebKit paints a moving box on its path, which it does not for a translate added to `none`', async () => {
  const film = resolve(dir, 'film.webm');
  const server = await serveModules({ '/': [page, 'text/html'], '/decode': [decode, 'text/html'],
    '/film.webm': [() => readFileSync(film), 'video/webm'], '/presence-glue.js': moduleRoute(PRESENCE) });
  const { url } = server;
  let webkit, chromium;
  try {
    webkit = await suiteBrowser('webkit');
    chromium = await suiteBrowser('chromium');
    const context = await webkit.newContext({ viewport: { width: 600, height: 140 }, recordVideo: { dir, size: { width: 600, height: 140 } } });
    const filmed = await context.newPage();
    await filmed.goto(`${url}/`);
    await filmed.waitForFunction(() => globalThis.ready);
    const played = await filmed.evaluate(() => globalThis.go());
    await context.close();
    await filmed.video().saveAs(film);

    const reader = await chromium.newPage();
    const read = async () => { await reader.goto(`${url}/decode`); return reader.evaluate(() => new Promise((done, fail) => {
      const video = document.querySelector('video'), g = document.querySelector('canvas').getContext('2d', { willReadFrequently: true });
      const exact = [], control = [], times = [];
      const leftmost = (y, match) => {
        const d = g.getImageData(0, y, 600, 1).data;
        for (let x = 0; x < 600; x++) if (match(d[x * 4], d[x * 4 + 1], d[x * 4 + 2])) return x;
        return null;
      };
      // Each frame once, by its time in the film
      const read = time => {
        if (times.length && time <= times.at(-1)) return;
        times.push(time);
        g.drawImage(video, 0, 0, 600, 140);
        exact.push(leftmost(40, (r, gr, b) => gr > 150 && r < 100 && b < 100));
        control.push(leftmost(100, (r, gr, b) => r > 150 && b > 150 && gr < 100));
      };
      const frame = (_, { mediaTime }) => { read(mediaTime); video.requestVideoFrameCallback(frame); };
      video.onended = () => done({ exact, control, times, end: video.currentTime });
      video.onerror = () => fail(new Error(`the film did not play: ${video.error?.message}`));
      // The first frame shows as the film loads, before playing presents any: it is read there. Then the film plays at
      // half speed, so that Chromium presents every frame it holds.
      const start = () => {
        read(video.currentTime);
        video.requestVideoFrameCallback(frame);
        video.playbackRate = 0.5;
        video.play().catch(fail);
      };
      if (video.readyState >= 2) start(); else video.addEventListener('loadeddata', start, { once: true });
    })); };
    // Whether Chromium presented every encoded frame: the first within half a frame of the start, none skipped after
    // it, and the last within a frame of the end. A busy machine can make it skip one, and then the film is read again.
    const coverage = rows => ({ first: rows.times[0] <= 0.02, gaps: rows.times.slice(1).filter((t, i) => t - rows.times[i] > 0.06).length, last: rows.end - rows.times.at(-1) <= 0.06 });
    const whole = rows => (c => c.first && !c.gaps && c.last)(coverage(rows));
    let rows = await read();
    for (let tries = 1; tries < 3 && !whole(rows); tries++) rows = await read();
    // Off the path: painted before the box's old place or past its new one
    const off = lefts => lefts.filter(x => x !== null && (x < 98 || x > 302)).length;
    // The module's move was paused before half way and finished once it went on
    expect({ pausedBeforeHalfWay: played.pausedAt < 750, finished: played.finished }).toEqual({ pausedBeforeHalfWay: true, finished: true });
    expect(coverage(rows)).toEqual({ first: true, gaps: 0, last: true });
    // The film sees WebKit paint a translate added to `none` off its path: else it could not see the module's either
    expect({ 'WebKit paints a translate added to none off its path': off(rows.control) > 0 }).toEqual({ 'WebKit paints a translate added to none off its path': true });
    // The module's box, from the frame it is first seen, is seen in every frame: at rest at 100, in at least two places
    // between, and at rest at 300, never off its path and never back on it (two points either way are the film's
    // compression)
    const seen = rows.exact.slice(rows.exact.findIndex(x => x !== null)), between = new Set(seen.filter(x => x > 102 && x < 298));
    // (Going back: more than two points short of the furthest it has been)
    let furthest = -Infinity;
    const back = seen.filter(x => { if (x === null) return false; furthest = Math.max(furthest, x); return x < furthest - 2; }).length;
    expect({ unseen: seen.filter(x => x === null).length, from: Math.abs(seen[0] - 100) <= 2, to: Math.abs(seen.at(-1) - 300) <= 2, between: between.size >= 2, off: off(seen), back })
      .toEqual({ unseen: 0, from: true, to: true, between: true, off: 0, back: 0 });
  } finally { await webkit?.close(); await chromium?.close(); server.close(); }
}, 60000);
