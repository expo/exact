import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

async function inPage(check) {
  const root = resolve(import.meta.dir, '../../..');
  const server = Bun.serve({ port: 0, async fetch(req) {
    const path = new URL(req.url).pathname;
    return path === '/' ? new Response('<!doctype html><body>', { headers: { 'content-type': 'text/html' } })
      : await Bun.file(resolve(root, path.slice(1))).exists() ? new Response(Bun.file(resolve(root, path.slice(1))))
      : new Response('not found', { status: 404 });
  } });
  const browser = await suiteBrowser('chromium', { needs: ['hevc'] });
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.port}/`);
    await check(page);
  } finally { await browser.close(); server.stop(true); }
}

test('the agent clip supplies the same timed frame to the viewfinder, reader and photo', () => inPage(async page => {
  const result = await page.evaluate(async () => {
    const { implementation } = await import('/packages/camera/web/index.js');
    let time = 500, frame, shown, tick;
    const logs = [], written = new Map();
    const entry = { kind: 'video', startedAt: 500,
      state: () => ({ readyState: 'live', enabled: true, muted: false, settings: {} }),
      owner: { picture(id, value) { frame = value; } }, frame: () => frame };
    const impl = implementation({ agent: true, now: () => time, log: s => logs.push(s),
      changed() {}, clocked: fn => { tick = fn; }, tracks: { lookup: () => entry,
        subscribe(id, consumer) { consumer.live(null); return () => {}; } },
      files: { write: async (path, bytes) => written.set(path, bytes) } });
    const color = async bitmap => {
      const canvas = new OffscreenCanvas(1, 1), ctx = canvas.getContext('2d');
      ctx.drawImage(await bitmap, 0, 0, 1, 1);
      const rgb = [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3);
      return rgb.indexOf(Math.max(...rgb));
    };
    await impl.preferred({ clip: { path: 'packages/camera/fixture/frames.mp4' } });
    impl.propsOf('finder', { srcObject: 'track' });
    impl.made('finder', { show(source, url) { shown = url; }, blank() { shown = null; } });
    impl.bound('finder', 'srcObject', 'app');
    impl.displayed('finder', true, 'app');
    const samples = [];
    for (const at of [500, 599, 600, 700, 800, 1600]) {
      time = at;
      tick?.();
      const photo = await impl.takePhoto({ track: 'track' });
      const bytes = written.get(photo.path);
      const captured = await createImageBitmap(new Blob([bytes], { type: photo.type }));
      samples.push({ width: photo.width, frame: await color(frame), photo: await color(captured),
        shown: await color(createImageBitmap(await (await fetch(shown)).blob())) });
      captured.close();
    }
    await impl.preferred({ picture: { path: 'packages/camera/fixture/card.jpg' }, clip: { path: 'packages/camera/fixture/frames.mp4' } });
    const conflict = logs.some(line => line.includes('camera.picture') && line.includes('camera.clip'));
    await impl.preferred({ picture: { path: 'packages/camera/fixture/card.jpg' } });
    const still = await impl.takePhoto({ track: 'track' });
    impl.destroy?.();
    return { samples, conflict, still: still.width, logs };
  });
  expect(result.samples).toEqual([0, 0, 1, 2, 0, 2].map(frame => ({ width: 16, frame, photo: frame, shown: frame })));
  expect(result.conflict).toBe(true);
  expect(result.still).toBeGreaterThan(16);
  expect(result.logs.filter(line => line.includes('clip') && line.includes('sha256'))).toHaveLength(1);
}), 30000);

test.each(['cleared', 'conflicting'])('%s preferences cancel a pending clip', change => inPage(async page => {
  const result = await page.evaluate(async change => {
    const { implementation } = await import('/packages/camera/web/index.js');
    const logs = [], written = new Map();
    const entry = { kind: 'video', state: () => ({ readyState: 'live', enabled: true, muted: false }) };
    const impl = implementation({ agent: true, changed() {}, log: s => logs.push(s),
      tracks: { lookup: () => entry }, files: { write: async (path, bytes) => written.set(path, bytes) } });
    const fetch = window.fetch;
    let release;
    const held = new Promise(resolve => { release = resolve; });
    window.fetch = async (...args) => { await held; return fetch(...args); };
    const loading = impl.preferred({ clip: { path: 'packages/camera/fixture/frames.mp4' } });
    await impl.preferred(change === 'cleared' ? {} : {
      clip: { path: 'packages/camera/fixture/frames.mp4' },
      picture: { path: 'packages/camera/fixture/card.jpg' },
    });
    release();
    await loading;
    window.fetch = fetch;
    const photo = await impl.takePhoto({ track: 't' });
    const card = new Uint8Array(await (await fetch('/packages/camera/fixture/card.jpg')).arrayBuffer());
    const bytes = written.get(photo.path);
    impl.destroy?.();
    return { same: bytes.length === card.length && bytes.every((b, i) => b === card[i]), logs };
  }, change);
  expect(result.same).toBe(true);
  expect(result.logs.some(line => line.includes('clip') && line.includes('sha256'))).toBe(false);
  if (change === 'conflicting') expect(result.logs.some(line => line.includes('cannot both be set'))).toBe(true);
}), 30000);

test('clip frames and loop duration follow the MP4 edit list', () => inPage(async page => {
  const result = await page.evaluate(async () => {
    const { decodeClip, clipFrame } = await import('/packages/camera/web/agent-clip.js');
    const path = 'packages/camera/fixture/trimmed.mp4';
    const video = document.createElement('video');
    const ready = new Promise((resolve, reject) => { video.requestVideoFrameCallback(resolve); video.onerror = reject; });
    document.body.append(video);
    video.src = path;
    await ready;
    const color = image => {
      const ctx = new OffscreenCanvas(1, 1).getContext('2d');
      ctx.drawImage(image, 0, 0, 1, 1);
      const rgb = [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3);
      return rgb.indexOf(Math.max(...rgb));
    };
    const expected = { duration: video.duration * 1000, colors: [color(video)] };
    // `seeked` can fire before the sought frame is presented; the frame callback comes with it.
    const sought = new Promise(resolve => video.requestVideoFrameCallback(resolve));
    video.currentTime = 0.15;
    await sought;
    expected.colors.push(color(video));
    const clip = await decodeClip(path), photos = [];
    for (const frame of clip.frames) {
      const photo = await createImageBitmap(frame.blob);
      photos.push(color(photo));
      photo.close();
    }
    const actual = { duration: clip.duration, colors: clip.frames.map(f => color(f.bitmap)),
      times: clip.frames.map(f => f.at), photos,
      loop: [0, 99, 100, 199, 200, 300, 400].map(at => color(clipFrame(clip, at).bitmap)) };
    clip.close();
    return { expected, actual };
  });
  expect(result.expected).toEqual({ duration: 200, colors: [1, 2] });
  expect(result.actual).toEqual({ ...result.expected, times: [0, 100], photos: [1, 2], loop: [1, 1, 2, 2, 1, 2, 1] });
}), 30000);

test('64-bit edits map partial frames, repeated media, gaps and still segments', () => inPage(async page => {
  const source = Buffer.from(await Bun.file(resolve(import.meta.dir, '../fixture/frames.mp4')).arrayBuffer());
  const edits = [[50, 120, 1], [75, 30, 1], [100, 60, 0], [25, -1, 1], [100, 120, 1]];
  const elst = Buffer.alloc(16 + edits.length * 20);
  elst.writeUInt32BE(elst.length, 0); elst.write('elst', 4); elst[8] = 1;
  elst.writeUInt32BE(edits.length, 12);
  for (const [i, [duration, media, rate]] of edits.entries()) {
    elst.writeBigUInt64BE(BigInt(duration), 16 + i * 20);
    elst.writeBigInt64BE(BigInt(media), 24 + i * 20);
    elst.writeInt32BE(rate * 65536, 32 + i * 20);
  }
  const at = source.indexOf('elst') - 4, size = source.readUInt32BE(at);
  for (const name of ['moov', 'trak', 'edts']) {
    const pos = source.indexOf(name) - 4;
    source.writeUInt32BE(source.readUInt32BE(pos) + elst.length - size, pos);
  }
  source.writeUInt32BE(1000, source.indexOf('mvhd') + 16);
  source.writeUInt32BE(350, source.indexOf('mvhd') + 20);
  source.writeUInt32BE(350, source.indexOf('tkhd') + 24);
  await page.route('**/edited.mp4', route => route.fulfill({ contentType: 'video/mp4',
    body: Buffer.concat([source.subarray(0, at), elst, source.subarray(at + size)]) }));
  const result = await page.evaluate(async () => {
    const { decodeClip, clipFrame } = await import('/packages/camera/web/agent-clip.js');
    const clip = await decodeClip('edited.mp4');
    const colors = [0, 49, 50, 124, 125, 224, 225, 249, 250, 349, 350].map(at => {
      const ctx = new OffscreenCanvas(1, 1).getContext('2d');
      ctx.drawImage(clipFrame(clip, at).bitmap, 0, 0, 1, 1);
      const rgb = [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3);
      return Math.max(...rgb) < 10 ? -1 : rgb.indexOf(Math.max(...rgb));
    });
    const result = { duration: clip.duration, times: clip.frames.map(f => f.at), colors };
    clip.close();
    return result;
  });
  expect(result).toEqual({ duration: 350, times: [0, 50, 100, 125, 225, 250],
    colors: [2, 2, 0, 1, 1, 1, -1, -1, 2, 2, 2] });
}), 30000);

test('clip bitmaps and photos follow the MP4 display transform', () => inPage(async page => {
  const result = await page.evaluate(async () => {
    const { decodeClip } = await import('/packages/camera/web/agent-clip.js');
    const path = 'packages/camera/fixture/rotated.mp4';
    const video = document.createElement('video');
    const ready = new Promise((resolve, reject) => { video.requestVideoFrameCallback(resolve); video.onerror = reject; });
    document.body.append(video);
    video.src = path;
    await ready;
    const sample = (image, width, height) => {
      const canvas = new OffscreenCanvas(2, 2), ctx = canvas.getContext('2d');
      ctx.drawImage(image, 0, 0, 2, 2);
      const pixels = ctx.getImageData(0, 0, 2, 2).data;
      const colors = [0, 4, 8, 12].map(at => {
        const rgb = [...pixels.slice(at, at + 3)];
        return rgb.indexOf(Math.max(...rgb));
      });
      return { width, height, colors };
    };
    const expected = sample(video, video.videoWidth, video.videoHeight);
    const clip = await decodeClip(path), frame = clip.frames[0];
    const photo = await createImageBitmap(frame.blob);
    const actual = [sample(frame.bitmap, frame.bitmap.width, frame.bitmap.height), sample(photo, photo.width, photo.height)];
    photo.close(); clip.close();
    return { expected, actual };
  });
  expect(result.expected).toEqual({ width: 32, height: 16, colors: [0, 0, 1, 1] });
  expect(result.actual).toEqual([result.expected, result.expected]);
}), 30000);
