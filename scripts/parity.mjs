// The parity comparator, generalised from scripts/svgparity.mjs (LLP 1055.000
// §5, LLP 1056 §4): a gallery app's `fx-*` boxes cropped from each host's
// screenshots by that host's own layout and compared with Chrome's. The
// SVG smoke keeps its own copy until the SVG stages 8–10 lane lands (QUEUE);
// the canvas smoke (scripts/canvasparity.mjs) uses this one.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { appleArtifacts, appleRunBinary } from '../host/apple/build.mjs';
import { linuxRebuild, ownVerb, rebuildCommand } from './agent-build.mjs';
import { linuxBinary, resolveApp } from './app.mjs';
import { crop, decodePng } from './png.mjs';

const ROOT = resolve(import.meta.dir, '..');

/** Report every missing requested native build before starting a parity capture. */
export function requireNativeBuilds(app, hosts, { device = false } = {}) {
  const a = resolveApp(app), missing = [];
  for (const host of new Set(hosts)) {
    const override = host === 'linux' ? 'EXACT_LINUX_BIN' : host === 'macos' ? 'EXACT_MAC_BIN' : null;
    const built = host === 'linux' ? (process.env.EXACT_LINUX_BIN ?? linuxBinary(a))
      : host === 'macos' ? (process.env.EXACT_MAC_BIN ?? appleRunBinary(a))
      : host === 'ios' ? appleArtifacts(a, { destination: device ? 'ios' : 'ios-simulator' }).bundle : null;
    if (built === null || existsSync(built)) continue;
    if (override && process.env[override] !== undefined) {
      missing.push(`  ${host}: ${override} names no file: ${built}`);
      continue;
    }
    const command = host === 'linux' ? linuxRebuild(a)
      : host === 'ios' && device ? (ownVerb(a, 'ios --device') ?? `bun host/apple/build.mjs --device ${a.crate('apple')}`)
      : rebuildCommand(a, host === 'ios' ? 'ios' : 'mac');
    missing.push(`  ${host}: run ${command} first`);
  }
  if (missing.length) throw new Error(`missing native builds for ${a.name}:\n${missing.join('\n')}`);
}

/** A gallery's web build, the JS target the app ships (LLP 1071), in its own dist (`target/web-dist-js/<app>`) so a
 * smoke leaves `host/web/dist` alone; built when missing, and again under `EXACT_PARITY_REBUILD=1`. */
export function webBuild(app) {
  const dist = resolve(ROOT, 'target/web-dist-js', app);
  if (process.env.EXACT_PARITY_REBUILD === '1' || !existsSync(resolve(dist, '.exact-build.json'))) {
    mkdirSync(dirname(dist), { recursive: true });
    const b = spawnSync('bun', [resolve(ROOT, 'host/web/build.mjs'), `${app}-web`, '--js'], { cwd: ROOT, env: { ...process.env, EXACT_WEB_DIST: dist }, stdio: 'inherit' });
    if (b.status !== 0) throw new Error(`the ${app} web build failed`);
  }
  return dist;
}

/** An RGBA image averaged down by an integer factor. */
export function shrink(img, k) {
  if (k === 1) return img;
  const width = Math.floor(img.width / k), height = Math.floor(img.height / k);
  const data = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    for (let c = 0; c < 4; c++) {
      let sum = 0;
      for (let j = 0; j < k; j++) for (let i = 0; i < k; i++) sum += img.data[((y * k + j) * img.width + x * k + i) * 4 + c];
      data[(y * width + x) * 4 + c] = Math.round(sum / (k * k));
    }
  }
  return { width, height, data };
}

/** Mean |Δ| over RGB and the share of pixels off by more than `band`, `b`
 * shifted by (dx, dy) over `a`'s interior (`slop` px of margin). */
export function compare(a, b, dx, dy, { slop, band }) {
  let sum = 0, off = 0, n = 0;
  for (let y = slop; y < a.height - slop; y++) for (let x = slop; x < a.width - slop; x++) {
    const i = (y * a.width + x) * 4, j = ((y + dy) * b.width + x + dx) * 4;
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a.data[i + c] - b.data[j + c]); sum += d; if (d > m) m = d; }
    if (m > band) off++;
    n++;
  }
  return { mean: sum / (3 * n), off: off / n };
}

/** The best registration of `b` over `a` within ±slop. */
export function register(a, b, { slop, band }) {
  let best = null;
  for (let dy = -slop; dy <= slop; dy++) for (let dx = -slop; dx <= slop; dx++) {
    const r = compare(a, b, dx, dy, { slop, band });
    if (!best || r.mean < best.mean) best = { ...r, dx, dy };
  }
  return best;
}

/** One screenshot of a session, in sRGB, with its device scale `k` and the
 * rows above the viewport (a window capture's title bar). */
export async function shot(s, host, dir, name) {
  const layout = await s.layout();
  let path = resolve(dir, `${host}-${name}.png`);
  await s.screenshot(path, host === 'macos');
  if (host === 'macos' || host === 'ios') {
    // A native capture is in the display's profile (Display P3 on a Mac);
    // Chrome's is sRGB.
    const srgb = resolve(dir, `${host}-${name}.srgb.png`);
    const r = spawnSync('sips', ['--matchTo', '/System/Library/ColorSync/Profiles/sRGB Profile.icc', path, '--out', srgb], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`sips: ${r.stderr}`);
    path = srgb;
  }
  const img = decodePng(readFileSync(path));
  const k = Math.round(img.width / layout.viewport.w);
  const top = Math.max(0, Math.round(img.height / k - layout.viewport.h));
  return { img, k, top, layout };
}

/** Every box whose testId matches `prefix`, cropped from a shot at 1 px per
 * point (`native` keeps the device scale), `slop` px of margin around it. */
export function boxes({ img, k, top, layout }, { prefix = 'fx-', slop, native = false }) {
  const scale = native ? k : 1;
  const full = native ? img : shrink(img, k);
  const out = {};
  for (const n of layout.nodes.filter((n) => n.testId?.startsWith(prefix))) {
    const x = Math.round(n.x * scale) - slop, y = Math.round((n.y + top) * scale) - slop;
    const w = Math.round(n.w * scale) + 2 * slop, h = Math.round(n.h * scale) + 2 * slop;
    out[n.testId] = x < 0 || y < 0 || x + w > full.width || y + h > full.height ? null : crop(full, x, y, w, h);
  }
  return out;
}
