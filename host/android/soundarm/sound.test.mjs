import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { androidToolchainEnv, loadAlignment, stageSoundLibrary, zipEntry, keptLibrary } from '../../../scripts/agent-android.mjs';

let env;
try { env = androidToolchainEnv(); } catch {}

test.skipIf(!env)('the Android sound arm packages as its own 16 KB aligned AAudio library', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-sound-apk-'));
  try {
    const library = stageSoundLibrary(resolve(dir, 'lib/arm64-v8a'), env);
    expect(loadAlignment(library)).toBeGreaterThanOrEqual(16384);
    const bin = resolve(env.ANDROID_NDK_HOME, 'toolchains/llvm/prebuilt', process.platform === 'darwin' ? 'darwin-x86_64' : 'linux-x86_64', 'bin');
    const symbols = spawnSync(resolve(bin, 'llvm-readelf'), ['-d', '-Ws', library], { encoding: 'utf8' });
    expect(symbols.status).toBe(0);
    expect(symbols.stdout).toContain('libaaudio.so');
    expect(symbols.stdout).toContain('Java_dev_exact_canvas_SoundNative_open');
    expect(symbols.stdout).toContain('AAudioStream_getTimestamp');
    // APK entries may be compressed (GPU packaging) or stored. Exercise the
    // same artifact comparison the app build applies after Gradle packages it.
    const packed = spawnSync('zip', ['-q', 'app.apk', 'lib/arm64-v8a/libexact_sound.so'], { cwd: dir });
    expect(packed.status).toBe(0);
    const entry = zipEntry(readFileSync(resolve(dir, 'app.apk')), 'lib/arm64-v8a/libexact_sound.so');
    expect(keptLibrary(readFileSync(library), entry, [])).toBeNull();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('the arm decodes PCM16, float and extensible WAVs into resampled stereo planes', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-sound-wav-'));
  try {
    const { writeFileSync } = require('node:fs');
    const source = resolve(dir, 'decode.c'), binary = resolve(dir, 'decode');
    writeFileSync(source, `#include "sound_wav.h"
#include <stdio.h>
#include <stdlib.h>
int main(int n, char **v) {
  if (n != 2) return 2;
  unsigned frames, channels; char reason[512];
  float *p = exact_sound_read_wav(v[1], 96000, &frames, &channels, reason, sizeof(reason));
  if (!p) { puts(reason); return 1; }
  printf("%u %u", frames, channels);
  for (unsigned i = 0; i < frames * 2; i++) printf(" %.6f", p[i]);
  free(p); return 0;
}`);
    const built = spawnSync('cc', ['-std=gnu11', '-Wall', '-Wextra', '-Werror', '-I', import.meta.dir,
      source, resolve(import.meta.dir, 'sound_wav.c'), '-lm', '-o', binary], { encoding: 'utf8' });
    expect(built.status).toBe(0);
    for (const format of [1, 3, 0xfffe]) {
      const floats = format !== 1, fmtSize = format === 0xfffe ? 40 : 16, size = floats ? 16 : 8;
      const bytes = Buffer.alloc(12 + 8 + fmtSize + 8 + size);
      bytes.write('RIFF'); bytes.writeUInt32LE(bytes.length - 8, 4); bytes.write('WAVEfmt ', 8);
      bytes.writeUInt32LE(fmtSize, 16); bytes.writeUInt16LE(format, 20); bytes.writeUInt16LE(1, 22);
      bytes.writeUInt32LE(48000, 24); bytes.writeUInt32LE(48000 * (floats ? 4 : 2), 28);
      bytes.writeUInt16LE(floats ? 4 : 2, 32); bytes.writeUInt16LE(floats ? 32 : 16, 34);
      if (format === 0xfffe) {
        bytes.writeUInt16LE(22, 36); bytes.writeUInt16LE(32, 38); bytes.writeUInt32LE(3, 44);
        Buffer.from([0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]).copy(bytes, 48);
      }
      const data = 20 + fmtSize;
      bytes.write('data', data); bytes.writeUInt32LE(size, data + 4);
      [0.5, 0, -0.5, 0.25].forEach((v, i) => floats ? bytes.writeFloatLE(v, data + 8 + i * 4) : bytes.writeInt16LE(v * 32768, data + 8 + i * 2));
      const file = resolve(dir, 'sound.wav'); writeFileSync(file, bytes);
      const run = spawnSync(binary, [file], { encoding: 'utf8' });
      expect(run.status).toBe(0);
      expect(run.stdout.trim().split(/\s+/).map(Number)).toEqual([8, 1,
        ...[0.5, 0.25, 0, -0.25, -0.5, -0.125, 0.25, 0.125],
        ...[0.5, 0.25, 0, -0.25, -0.5, -0.125, 0.25, 0.125]]);
      writeFileSync(file, bytes.subarray(0, bytes.length - 1));
      expect(spawnSync(binary, [file]).status).toBe(1);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
