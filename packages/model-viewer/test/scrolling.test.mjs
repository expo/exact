import { beforeAll, expect, test } from 'bun:test';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../../scripts/agent.mjs';
import { ensureShells } from '../../../scripts/app-shells.mjs';

// LLP 1133.039 D15: cache loading on all four hosts; scrolling on web, macOS and iOS.
const root = resolve(import.meta.dir, '../../..');
const fixture = resolve(import.meta.dir, 'fixture');
const app = resolve(root, 'target/model-scroll-fixture');
const host = process.env.EXACT_MODEL_HOST ?? 'web';
const env = { ...process.env, EXACT_FEEDBACK: '0', EXACT_IDENTITY: '-', EXACT_SHOW_SIMULATOR: '0', EXACT_GPU_RENDER: '1' };

async function run(args, cwd = app) {
  const child = Bun.spawn(['bun', ...args], { cwd, env, stdout: 'pipe', stderr: 'inherit' });
  const output = await new Response(child.stdout).text();
  if (await child.exited !== 0) throw Error(`bun ${args.join(' ')} failed:\n${output}`);
}

beforeAll(async () => {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  if (!existsSync(resolve(app, 'app.json')))
    await run([resolve(root, 'scripts/exact.mjs'), 'new', app, '--with', 'mac,ios,linux'], root);
  for (const file of readdirSync(fixture).filter(f => /\.(contract|ts)$/.test(f) || f === 'assets'))
    cpSync(resolve(fixture, file), resolve(app, file), { recursive: true });
  rmSync(resolve(app, 'app.test.contract'), { force: true });
  const manifest = JSON.parse(readFileSync(resolve(app, 'package.json'), 'utf8'));
  manifest.dependencies = { '@exact/model-viewer': `file:${resolve(import.meta.dir, '..')}` };
  writeFileSync(resolve(app, 'package.json'), JSON.stringify(manifest, null, 2));
  await run(['install']);
  ensureShells(app);
  const build = { web: ['web-build'], linux: ['linux'], macos: ['mac'], ios: ['ios', '--no-install'] }[host];
  if (!build) throw Error(`EXACT_MODEL_HOST: unknown host ${host}`);
  await run(['exact.mjs', ...build]);
}, 600_000);

test.each(['/', '/scroll'])(`${host}: cached model${host === 'linux' ? '' : ' and camera controls inside a scrolling page'} from %s`, async url => {
  const s = await open({ host, app, url, size: [390, 844], env,
    storage: `model-scroll-${process.pid}`, touch: host === 'ios' ? 'drag' : 'agent' });
  try {
    await s.clock('settle');
    if (url === '/') {
      await s.tap('scrolling-page');
      await s.clock('settle');
    }
    expect((await s.find('model-scroll')).type).toBe('ScrollView');
    await s.clock('until state resources.viewer.presented == true within 5000');
    const viewer = () => s.state().then(state => state.resources.viewer);
    expect(await viewer()).toMatchObject({ loaded: true, error: '', triangles: 1,
      src: 'app:/cache/model-viewer/triangle.glb', nodes: [{ name: 'Cached triangle', parent: '' }] });
    if (host === 'linux') {
      await s.tap('controlled-model', { at: [140, 120] });
      await s.clock('settle');
      expect((await s.find('scroll-picked')).props.text).toBe('Cached triangle');
      return;
    }
    const box = async target => (await s.layout(target)).node.space.viewport;
    const y = async target => (await box(target)).y;
    const startY = await y('controlled-model');
    const startOrbit = (await viewer()).cameraOrbit;
    await s.tap('controlled-model', { drag: { dx: 45, dy: -35, over: 200 } });
    await s.clock('settle');
    expect(await y('controlled-model')).toBeCloseTo(startY, 0);
    expect((await viewer()).cameraOrbit).not.toBe(startOrbit);
    const draggedOrbit = (await viewer()).cameraOrbit;
    await s.tap('controlled-model', { wheel: [0, 30] });
    await s.clock('settle');
    expect(await y('controlled-model')).toBeCloseTo(startY, 0);
    expect((await viewer()).cameraOrbit).not.toBe(draggedOrbit);

    // The gutter lies outside the canvas, in the same scroll container.
    await s.tap('model-scroll', { drag: { dx: 0, dy: -100, from: [345, 500], over: 250 } });
    await s.clock('settle');
    expect(await y('controlled-model')).toBeLessThan(startY - 10);

    // Each mode mounts its own canvas: wants_input is read only at mount (LLP 1133.039 D9).
    await s.tap('passive-mode');
    await s.clock('settle');
    await s.clock('until state resources.viewer.presented == true within 5000');
    expect((await viewer()).loaded).toBe(true);
    const passive = await box('passive-model');
    await s.tap('model-scroll', { wheel: [0, passive.y - 280], at: [345, 400] });
    await s.clock('settle');
    const passiveY = await y('passive-model');
    const beforePassive = (await viewer()).cameraOrbit;
    await s.tap('passive-model', { drag: { dx: 0, dy: -60, over: 250 } });
    await s.clock('settle');
    expect(await y('passive-model')).toBeLessThan(passiveY - 10);
    const afterDragY = await y('passive-model');
    await s.tap('passive-model', { wheel: [0, 40] });
    await s.clock('settle');
    expect(await y('passive-model')).toBeLessThan(afterDragY - 10);
    expect((await viewer()).cameraOrbit).toBe(beforePassive);
    await s.tap('passive-model', { at: [140, 120] });
    await s.clock('settle');
    expect((await s.find('scroll-picked')).props.text).toBe('Cached triangle');
    await s.tap('passive-model', { at: [1, 1] });
    await s.clock('settle');
    expect((await s.find('scroll-picked')).props.text).toBe('');
  } finally {
    await s.close();
  }
}, 120_000);
