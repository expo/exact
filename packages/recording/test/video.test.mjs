import { expect, test } from 'bun:test';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, makeFixture, run as fixtureRun } from './fixture.mjs';

const run = (args) => {
  const r = spawnSync('xcrun', args, { cwd: root, encoding: 'utf8', timeout: 120000 });
  expect(r.status, r.stderr).toBe(0);
};

test.skipIf(process.platform !== 'darwin')('Apple video recording writes a playable MP4 on the session clock', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/recording-video-'));
  try {
    run(['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module',
      '-emit-module-path', join(out, 'ExactPackage.swiftmodule'), 'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run(['swiftc', '-parse-as-library', '-swift-version', '5', '-I', out, '-L', out, '-lExactPackage',
      'packages/media-capture/apple/MediaCapture.swift', 'packages/media-capture/apple/VideoCapture.swift',
      'packages/recording/apple/Recording.swift', 'packages/recording/apple/VideoRecorder.swift', 'packages/recording/test/Video.swift', '-o', join(out, 'video')]);
    const results = ['', 'delayed', 'rate'].map(scenario => spawnSync(join(out, 'video'),
      [join(out, 'files'), ...(scenario ? [scenario] : [])], { encoding: 'utf8', timeout: 20000 }));
    expect(results.map(r => r.status), results.map(r => r.stderr).join('\n')).toEqual([0, 0, 0]);
    for (const r of results) expect(r.stdout).toContain('PASS Apple');
  } finally { rmSync(out, { recursive: true, force: true }); }
}, 120000);

test.each([['video/mp4', false], ['video/webm', false], ['video/mp4', true], ['video/webm', true]])(
  'the browser reports the finished %s after pause, constraints and resume (initial frames: %s)', async (mimeType, initialFrames) => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<video autoplay muted playsinline></video>', { headers: { 'content-type': 'text/html' } });
    if (['/index.js', '/metadata.js'].includes(path)) return new Response(Bun.file(join(root, 'packages/recording/web', path.slice(1))));
    return new Response('missing', { status: 404 });
  } });
  const browser = await suiteBrowser('chromium', { args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] });
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.port}`);
    const recorded = await page.evaluate(async ({ mimeType, initialFrames }) => {
      const { implementation } = await import('/index.js');
      const stream = await navigator.mediaDevices.getUserMedia({ video: { width: 320, height: 240, frameRate: 15 } });
      const camera = stream.getVideoTracks()[0], preview = document.querySelector('video');
      preview.srcObject = stream;
      const chunks = [];
      let id, released = 0, finish;
      const done = new Promise(resolve => { finish = resolve; });
      const impl = implementation({ agent: false, now: () => performance.now(),
        changed() { if (id) impl.mediaRecorderState({ recorder: id }).then(state => { if (state.result || state.error) finish(state); }); },
        tracks: {
          lookup: id => id === 'stream' ? { kind: 'stream', tracks: ['camera'] } : { kind: 'video' },
          subscribe(_, consumer) { consumer.live(camera); return () => released++; },
        },
        files: { async read() { return new Blob(chunks).arrayBuffer(); }, async write(_, bytes) { chunks.length = 0; chunks.push(bytes); }, async append(_, bytes) { chunks.push(bytes); } },
      });
      const started = await impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/clip', mimeType, maxDuration: 1200 });
      id = started.id;
      if (!id) throw new Error(JSON.stringify(started));
      if (initialFrames) for (let i = 0; i < 2; i++) await new Promise(resolve => preview.requestVideoFrameCallback(resolve));
      await impl.pauseMediaRecorder({ recorder: id });
      await camera.applyConstraints({ width: 640, height: 480, frameRate: 5 });
      await new Promise(resolve => preview.requestVideoFrameCallback(resolve));
      await impl.resumeMediaRecorder({ recorder: id });
      const state = await done;
      if (state.error) throw new Error(state.error);
      const file = new Blob(chunks, { type: state.result.mimeType }), video = document.createElement('video');
      video.src = URL.createObjectURL(file);
      await new Promise((resolve, reject) => { video.onloadeddata = resolve; video.onerror = () => reject(new Error('the recorded video did not decode')); });
      if (!Number.isFinite(video.duration)) {
        video.currentTime = 1e10;
        await new Promise(resolve => { video.onseeked = resolve; });
      }
      const value = { state, duration: video.duration * 1000, bytes: file.size, width: video.videoWidth, height: video.videoHeight,
        preview: preview.srcObject === stream && camera.readyState, released };
      stream.getTracks().forEach(t => t.stop());
      URL.revokeObjectURL(video.src);
      return value;
    }, { mimeType, initialFrames });
    expect(recorded.state.reason).toBe('max-duration');
    expect(recorded.state.error).toBeNull();
    expect(recorded.bytes).toBeGreaterThan(0);
    expect(recorded.state.result).toMatchObject({ size: recorded.bytes, width: recorded.width, height: recorded.height, duration: recorded.duration });
    expect(recorded.state.result.frameRate).toBeGreaterThan(3);
    expect(recorded.state.result.frameRate).toBeLessThan(12);
    expect(recorded.preview).toBe('live');
    expect(recorded.released).toBe(1);
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test('an agent drive records a camera stream without a microphone grant and publishes the bounded result', () => {
  const { app } = makeFixture({ directory: join(root, 'target'), dependencies: { '@exact/camera': `file:${root}/packages/camera` } });
  try {
    writeFileSync(join(app, 'app.contract'), readFileSync(join(import.meta.dir, 'fixture/video.contract')));
    writeFileSync(join(app, 'app.ts'), `export const grants = 'device.camera purpose.camera\\nfs.write app:/data/takes'; export const answer = () => { throw new Error('no data'); };`);
    writeFileSync(join(app, 'strings/en.json'), JSON.stringify({ 'purpose.camera': 'Record a guided clip.' }));
    const env = { ...process.env, EXACT_APP_DIR: app, HERMES_LEAN_SYS_OFFLINE: '1' };
    fixtureRun('bun', ['host/web/build.mjs', 'recfixture', '--render', 'none'], root, env);
    const output = fixtureRun('bun', ['scripts/agent.mjs', 'web', '--app', 'recfixture', '--json', '--storage', 'recording-video', '--reset-storage',
      '--permission', 'camera=granted', 'clock settle', 'tap open', 'clock settle', 'tap record', 'clock settle',
      'clock +5000', 'clock settle', 'state recording', 'state extensions'], root, env);
    const values = output.split('\n').filter(line => line.startsWith('{')).map(line => JSON.parse(line));
    const recording = values.at(-2), extensions = values.at(-1);
    expect(recording).toMatchObject({ state: 'inactive', reason: 'max-duration', duration: 5000, error: null,
      result: { path: 'app:/data/takes/clip.mp4', mimeType: 'video/mp4', duration: 300, width: 16, height: 16, frameRate: 10 } });
    expect(recording.result.size).toBeGreaterThan(0);
    expect(extensions['media-capture'].implementation.videoTracks[0]).toMatchObject({ readyState: 'live', lease: true, consumers: ['app'] });
  } finally { rmSync(join(app, '..'), { recursive: true, force: true }); }
}, 240000);
