// Android text qualification. Run on an emulator assigned to this job:
// ANDROID_SERIAL=emulator-… bun test host/android/reference/advances.test.mjs
import { expect, test } from 'bun:test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { decodePng } from '../../../scripts/png.mjs';

const serial = process.env.ANDROID_SERIAL;
const enabled = /^emulator-\d+$/.test(serial ?? '');
const root = resolve(import.meta.dir, '../../..');
const specimen = 'apps/material-specimens/glyph-advances.contract';
const pkg = 'com.exact.material_specimens';

test.skipIf(!enabled)('painted line widths follow the emulator’s own text', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-glyphs-'));
  const tool = join(process.env.ANDROID_HOME ?? join(homedir(), 'Library/Android/sdk'), 'platform-tools/adb');
  const adb = (...args) => execFileSync(tool, ['-s', serial, ...args], { encoding: 'utf8', timeout: 30000 });
  const remote = '/data/local/tmp/exact-glyph-advances.plan';
  const run = (bin, args) => execFileSync(bin, args, { cwd: root, encoding: 'utf8', timeout: 600000, maxBuffer: 32 << 20 });
  const wait = async (read, description) => {
    const end = Date.now() + 15000;
    while (Date.now() < end) {
      const value = read();
      if (value) return value;
      await Bun.sleep(80);
    }
    throw new Error('Timed out waiting for ' + description);
  };
  try {
    const plan = join(dir, 'glyphs.plan');
    run('cargo', ['run', '--quiet', '-p', 'contract', '--bin', 'contract', '--', 'build', specimen, '-o', plan]);
    const apk = process.env.EXACT_ANDROID_BIN ?? run('bun', ['scripts/agent-android.mjs', 'build', 'material-specimens', '--apk', '--no-install']).trim().split('\n').at(-1);
    adb('install', '-r', apk);
    adb('push', plan, remote);
    adb('shell', 'am', 'force-stop', pkg);
    adb('shell', 'am', 'start', '-W', '-f', '0x10008000', '-n', pkg + '/dev.exact.app.MainActivity', '--es', 'x_plan', remote);
    const pid = await wait(() => adb('shell', 'pidof ' + pkg + ' || true').trim(), 'the specimen process');
    const logs = () => adb('logcat', '-d', '--pid=' + pid, '-v', 'raw', '-s', 'ExactAdvances:I');
    const latest = () => logs().trim().split('\n').filter(line => line.startsWith('{')).at(-1);
    await wait(latest, 'the first reference layout');
    const xml = '/data/local/tmp/exact-glyph-advances.xml';
    adb('shell', 'uiautomator', 'dump', xml);
    const bounds = adb('shell', 'cat', xml).match(/<node [^>]*text="Next sample"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    if (!bounds) throw new Error('Missing Next sample button');
    const tap = [1, 2].map(i => String(Math.floor((Number(bounds[i]) + Number(bounds[i + 2])) / 2)));
    const failures = [];
    const api = Number(adb('shell', 'getprop', 'ro.build.version.sdk').trim());
    console.log('API ' + api + ', ' + adb('shell', 'wm', 'density').trim().replaceAll('\n', ', '));
    const clusters = adb('logcat', '-d', '--pid=' + pid, '-v', 'raw', '-s', 'ExactClusters:I')
      .trim().split('\n').filter(line => line.startsWith('{')).map(JSON.parse);
    // ClusterProbe's ccmp expands A to eight Bs, fi ligates, the acute
    // attaches to B at 0.25 em, and decomposed e + acute composes to é. GPOS shifts C by 0.125 em.
    // D and G attach across seven pads. G’s base gets its advance from GPOS.
    // P attaches a mark with a positive advance. Only Android’s device-size positioning moves X.
    expect(clusters.map(r => r.text)).toEqual(['A', 'fi', 'B\u0301', 'e\u0301', 'BCB', 'D', 'G', 'U', 'J', 'P', ...(api >= 31 ? ['BXB'] : [])]);
    expect(clusters.map(r => r.ids)).toEqual([Array(8).fill(2), [5], [2, 7], [8], [2, 9, 2], [2, ...Array(7).fill(11), 7],
      [12, ...Array(7).fill(11), 7], [12, 15], [17, ...Array(7).fill(11), 7], [2, ...Array(7).fill(11), 20], ...(api >= 31 ? [[2, 13, 2]] : [])]);
    for (const cluster of clusters) {
      console.log('cluster ' + JSON.stringify(cluster));
      // J's base and first pad have coincident ink.
      const inkOrigins = cluster.text === 'J' ? cluster.x.filter((_, i) => i !== 1) : cluster.x;
      expect(inkOrigins.length).toBe(cluster.ink.length);
      expect(Math.abs(cluster.advances.reduce((sum, w) => sum + w, 0) - cluster.nativeWidth)).toBeLessThanOrEqual(.01);
      // The G base’s nominal width is zero. Its GPOS advance stays fractional
      // in Android; on API 29 the advancing pads share the cluster width proportionally.
      inkOrigins.forEach((x, i) => {
        if (api >= 31 || cluster.text !== 'G' || i === 0 || i === cluster.x.length - 1)
          expect(Math.abs(x - cluster.ink[i])).toBeLessThanOrEqual(.5);
      });
      if (cluster.text === 'P') expect(cluster.advances.at(-1)).toBeGreaterThan(0);
      if (api < 31 && cluster.text === 'U')
        expect(Math.abs(cluster.x[1] - cluster.nativeWidth)).toBeLessThan(.001);
      if (api < 31 && cluster.text === 'G') {
        const units = [1081, ...Array(7).fill(550), 0];
        cluster.advances.forEach((w, i) => {
          expect(Math.abs(w - cluster.nativeWidth * units[i] / (1081 + 7 * 550))).toBeLessThan(.001);
        });
      }
      if (api >= 31) {
        expect(cluster.ids).toEqual(cluster.nativeIds);
        cluster.x.forEach((x, i) => expect(Math.abs(x - cluster.nativeX[i])).toBeLessThanOrEqual(.5));
      }
    }
    expect(clusters[2].advances[1]).toBe(0);
    expect(clusters[5].advances.at(-1)).toBe(0);
    for (let i = 0; i < 85; ++i) {
      const log = latest();
      const row = JSON.parse(log);
      console.log(JSON.stringify(row));
      if (row.sp !== [12,14,16,20,28][Math.floor(i / 17)]) failures.push(row);
      if (row.room < 0 && (i % 17 < 7 || [13,14,15].includes(i % 17))) {
        if (row.exact.length !== 2 || Math.abs(row.exact[0] - (row.sameFace ?? row.textView)[0]) > .5 ||
            Math.abs(row.exact[0] - row.editText[0]) > .5 || !(row.caretError <= .5) || (api >= 31 && !(row.glyphError <= .5))) failures.push(row);
      }
      if ([8, 16].includes(i % 17) && Math.abs(row.exact[0] - row.textView[0]) > (i % 17 === 8 ? 1.001 : 2.001)) failures.push(row);
      if (api < 31 && row.room < 0 && (i % 17 < 7 || i % 17 === 15) && !(row.outlineError <= .5)) failures.push(row);
      if (row.tabular && (row.digitWidths.length !== 10 ||
          row.digitWidths.some((w, n) => Math.abs(w - row.nativeDigits[n]) > .01))) failures.push(row);
      if (row.room === 368 && (row.textView.length === 2) !== (row.exact.length === 2)) failures.push(row);
      if (i % 17 < 3 && (Math.abs(row.exact[0] - row.compose[0]) > 1 ||
          (api >= 31 && !Number.isFinite(row.glyphError)))) failures.push(row);
      if (row.room > 0) {
        const ends = lines => lines.filter((_, n) => n % 2 === 1);
        expect(ends(row.exact)).toEqual(ends(row.simple));
        console.log('breaks ' + JSON.stringify({ sp: row.sp, room: row.room, exact: ends(row.exact),
          textView: ends(row.textView), simple: ends(row.simple), compose: ends(row.compose) }));
      }
      if (i === 37) {
        // The virtual label and native TextView are the first and last text
        // nodes for this value. Their ink is in the actual on-screen frame.
        adb('shell', 'uiautomator', 'dump', xml);
        const boxes = [...adb('shell', 'cat', xml).matchAll(/<node\b[^>]+>/g)].map(m => m[0])
          .filter(node => node.includes('text="0123456789"') && node.includes('class="android.widget.TextView"'))
          .map(node => node.match(/bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/).slice(1).map(Number));
        expect(boxes.length).toBeGreaterThanOrEqual(2);
        const image = decodePng(execFileSync(tool, ['-s', serial, 'exec-out', 'screencap', '-p'], { maxBuffer: 32 << 20 }));
        const ink = ([left, top, right, bottom]) => {
          const background = 4 * (top * image.width + right - 1);
          const columns = [];
          for (let x = left; x < right; x++) {
            for (let y = top; y < bottom; y++) {
              const p = 4 * (y * image.width + x);
              if ([0, 1, 2].reduce((v, c) => v + Math.abs(image.data[p + c] - image.data[background + c]), 0) > 180) {
                columns.push(x - left);
                break;
              }
            }
          }
          expect(columns.length).toBeGreaterThan(0);
          return [columns[0], columns.at(-1)];
        };
        const exactInk = ink(boxes[0]), nativeInk = ink(boxes.at(-1));
        console.log('mounted digit ink ' + JSON.stringify({ exactInk, nativeInk }));
        expect(Math.max(...exactInk.map((x, n) => Math.abs(x - nativeInk[n])))).toBeLessThanOrEqual(2);
      }
      if (i + 1 < 85) {
        adb('shell', 'input', 'tap', ...tap);
        await wait(() => latest() !== log, 'the next sample after ' + row.sample);
      }
    }
    console.log(await wait(() => logs().match(/documentNs=.*/)?.[0], 'the long-document layout'));
    expect(failures).toEqual([]);
  } finally {
    adb('shell', 'am', 'force-stop', pkg);
    adb('shell', 'rm', '-f', remote, '/data/local/tmp/exact-glyph-advances.xml');
    rmSync(dir, { recursive: true, force: true });
  }
}, 720000);
