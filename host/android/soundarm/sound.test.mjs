import { test, expect, beforeAll, afterAll } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { zipSync } from 'fflate';
import wavCases from '../../../contract/lower/tests/fixtures/sounds.json';
import { androidJavaEnv, androidToolchainEnv, loadAlignment, soundLibraryCommand, stageSoundLibrary, zipEntry, keptLibrary } from '../../../scripts/agent-android.mjs';

let env;
try { env = androidToolchainEnv(); } catch {}

test('Windows compiles the sound arm with clang.exe and an explicit Android target, without a shell', () => {
  const cc = 'C:\\Android SDK\\ndk\\toolchains\\llvm\\prebuilt\\windows-x86_64\\bin\\aarch64-linux-android29-clang.cmd';
  const command = soundLibraryCommand('C:\\output dir\\libexact_sound.so', { CC_aarch64_linux_android: cc });
  expect(command[0]).toBe(cc.replace('aarch64-linux-android29-clang.cmd', 'clang.exe'));
  expect(command).toContain('--target=aarch64-linux-android29');
  expect(command.slice(-2)).toEqual(['-o', 'C:\\output dir\\libexact_sound.so']);
});

function readelf(ndk, platform = process.platform) {
  const bin = resolve(ndk, 'toolchains/llvm/prebuilt', { darwin: 'darwin-x86_64', win32: 'windows-x86_64' }[platform] ?? 'linux-x86_64', 'bin');
  return resolve(bin, platform === 'win32' ? 'llvm-readelf.exe' : 'llvm-readelf');
}

test('packaging uses the Windows NDK readelf executable', () => {
  expect(readelf('/ndk', 'win32')).toBe(resolve('/ndk/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-readelf.exe'));
});

test.skipIf(!env)('the Android sound arm packages as its own 16 KB aligned AAudio library', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-sound-apk-'));
  try {
    const library = stageSoundLibrary(resolve(dir, 'lib/arm64-v8a'), env);
    expect(loadAlignment(library)).toBeGreaterThanOrEqual(16384);
    const symbols = spawnSync(readelf(env.ANDROID_NDK_HOME), ['-d', '-Ws', library], { encoding: 'utf8' });
    expect(symbols.status).toBe(0);
    expect(symbols.stdout).toContain('libaaudio.so');
    expect(symbols.stdout).toContain('Java_dev_exact_canvas_SoundNative_open');
    expect(symbols.stdout).toContain('AAudioStream_getTimestamp');
    // APK entries may be compressed (GPU packaging) or stored. Exercise the
    // same artifact comparison the app build applies after Gradle packages it.
    const bytes = readFileSync(library), name = 'lib/arm64-v8a/libexact_sound.so';
    for (const level of [0, 6]) {
      const entry = zipEntry(Buffer.from(zipSync({ [name]: bytes }, { level })), name);
      expect(keptLibrary(bytes, entry, [])).toBeNull();
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

let decodeDir, decoder;
beforeAll(() => {
  if (process.platform === 'win32') return;
  decodeDir = mkdtempSync(resolve(tmpdir(), 'exact-sound-wav-'));
  const source = resolve(decodeDir, 'decode.c');
  decoder = resolve(decodeDir, 'decode');
  writeFileSync(source, `#include "sound_wav.h"
#include <stdio.h>
#include <stdlib.h>
int main(int n, char **v) {
  if (n != 3) return 2;
  unsigned frames, channels; char reason[512];
  float *p = exact_sound_read_wav(v[1], atoi(v[2]), &frames, &channels, reason, sizeof(reason));
  if (!p) { puts(reason); return 1; }
  printf("%u %u", frames, channels);
  for (unsigned i = 0; i < frames * 2; i++) printf(" %.6f", p[i]);
  free(p); return 0;
}`);
  const built = spawnSync('cc', ['-std=gnu11', '-Wall', '-Wextra', '-Werror', '-I', import.meta.dir,
    source, resolve(import.meta.dir, 'sound_wav.c'), '-lm', '-o', decoder], { encoding: 'utf8' });
  expect({ status: built.status, error: built.stderr }).toEqual({ status: 0, error: '' });
});
afterAll(() => { if (decodeDir) rmSync(decodeDir, { recursive: true, force: true }); });

test.skipIf(process.platform === 'win32').each(wavCases)('the compiler and arm agree on $name', fixture => {
  const file = resolve(decodeDir, 'sound.wav');
  writeFileSync(file, Buffer.from(fixture.hex, 'hex'));
  for (const multiplier of [1, 2]) {
    const run = spawnSync(decoder, [file, String((fixture.rate ?? 48000) * multiplier)], { encoding: 'utf8' });
    expect(run.status).toBe(fixture.frames === undefined ? 1 : 0);
    if (fixture.frames === undefined) continue;
    const samples = fixture.samples.flatMap((a, i, values) => multiplier === 1 ? [a] : [a, (a + (values[i + 1] ?? 0)) / 2]);
    expect(run.stdout.trim().split(/\s+/).map(Number)).toEqual([
      fixture.frames * multiplier, fixture.channels, ...samples, ...samples,
    ]);
  }
});

// AAudio is Android-only. A synchronous stream stand-in lets the host JVM
// exercise the real JNI string boundary, decoder and mixer without a device.
const java = androidJavaEnv();
test.skipIf(process.platform === 'win32' || !java?.JAVA_HOME || !existsSync(resolve(java.JAVA_HOME, 'include/jni.h')))(
  'JNI isolates failed assets, plays Unicode assets, cancels future voices and reopens output', () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-sound-jni-'));
    try {
      mkdirSync(resolve(dir, 'aaudio'));
      writeFileSync(resolve(dir, 'aaudio/AAudio.h'), `#include <stdint.h>
#include <stdlib.h>
typedef int32_t aaudio_result_t;
typedef int32_t aaudio_data_callback_result_t;
typedef struct AAudioStream { int unused; } AAudioStream;
typedef struct AAudioStreamBuilder { int unused; } AAudioStreamBuilder;
#define AAUDIO_OK 0
#define AAUDIO_DIRECTION_OUTPUT 0
#define AAUDIO_FORMAT_PCM_FLOAT 2
#define AAUDIO_SHARING_MODE_SHARED 1
#define AAUDIO_PERFORMANCE_MODE_LOW_LATENCY 12
#define AAUDIO_CALLBACK_RESULT_CONTINUE 0
typedef aaudio_data_callback_result_t (*Render)(AAudioStream *, void *, void *, int32_t);
static Render callback; static void *callback_context;
static int AAudio_createStreamBuilder(AAudioStreamBuilder **b) { *b = calloc(1, sizeof(**b)); return 0; }
static void AAudioStreamBuilder_delete(AAudioStreamBuilder *b) { free(b); }
#define SETTER(name) static void AAudioStreamBuilder_set##name(AAudioStreamBuilder *b, int n) { (void)b; (void)n; }
SETTER(Direction) SETTER(Format) SETTER(ChannelCount) SETTER(SharingMode) SETTER(PerformanceMode) SETTER(Usage) SETTER(ContentType)
static void AAudioStreamBuilder_setDataCallback(AAudioStreamBuilder *b, Render f, void *ctx) { (void)b; callback = f; callback_context = ctx; }
static void AAudioStreamBuilder_setErrorCallback(AAudioStreamBuilder *b, void (*f)(AAudioStream *, void *, int32_t), void *ctx) { (void)b; (void)f; (void)ctx; }
static int AAudioStreamBuilder_openStream(AAudioStreamBuilder *b, AAudioStream **s) { (void)b; *s = calloc(1, sizeof(**s)); return 0; }
static int AAudioStream_close(AAudioStream *s) { free(s); return 0; }
#define GETTER(name, value) static int32_t AAudioStream_##name(AAudioStream *s) { (void)s; return value; }
GETTER(getSampleRate, 48000) GETTER(getChannelCount, 2) GETTER(getFormat, 2)
GETTER(requestStart, 0) GETTER(requestStop, 0) GETTER(getXRunCount, 0) GETTER(getBufferSizeInFrames, 48)
static int64_t AAudioStream_getFramesWritten(AAudioStream *s) { (void)s; return 0; }
static int AAudioStream_getTimestamp(AAudioStream *s, int clock, int64_t *frame, int64_t *nanos) { (void)s; (void)clock; *frame = 0; *nanos = 1000000000; return 0; }
static const char *AAudio_convertResultToText(int result) { (void)result; return "test stream"; }
`);
      const mixer = resolve(import.meta.dir, '../../apple/soundarm');
      const source = resolve(dir, 'arm.c');
      writeFileSync(source, `#include "sound_arm.c"
JNIEXPORT jfloat JNICALL Java_dev_exact_canvas_SoundNative_render(JNIEnv *env, jclass klass) {
  (void)env; (void)klass; float data[96]; callback(NULL, callback_context, data, 48);
  float peak = 0; for (int i = 0; i < 96; i++) peak = fmaxf(peak, fabsf(data[i]));
  return peak;
}
`);
      const library = resolve(dir, process.platform === 'darwin' ? 'libsound.dylib' : 'libsound.so');
      const built = spawnSync('cc', ['-std=gnu11', '-Wall', '-Wextra', '-Werror', '-fPIC', '-shared',
        '-I', dir, '-I', import.meta.dir, '-I', mixer, '-I', resolve(java.JAVA_HOME, 'include'),
        '-I', resolve(java.JAVA_HOME, 'include', process.platform === 'darwin' ? 'darwin' : 'linux'),
        source, resolve(import.meta.dir, 'sound_wav.c'), resolve(mixer, 'sound_mixer.c'), '-lm', '-o', library], { encoding: 'utf8' });
      expect(built.stderr).toBe('');
      expect(built.status).toBe(0);
      const entry = resolve(dir, 'SoundNative.java');
      writeFileSync(entry, `package dev.exact.canvas;
public class SoundNative {
  static native long open(String[] files, int usage, int content, boolean[] decoded);
  static native boolean op(long arm, int kind, long id, int sound, double at, float gain, float rate, float pan, double ramp);
  static native double[] counts(long arm);
  static native void close(long arm);
  static native float render();
  public static void main(String[] args) {
    System.load(args[0]);
    boolean[] decoded = new boolean[3];
    long arm = open(new String[]{args[1], args[2], args[3]}, 1, 2, decoded);
    if (!decoded[0] || decoded[1] || !decoded[2]) throw new AssertionError("wrong decode results");
    if (arm == 0) throw new AssertionError("no arm");
    try {
      if (!op(arm, 1, 99, 1, 1, 1, 1, 0, 0)) throw new AssertionError("bad cue blocked the queue");
      if (!op(arm, 1, 100, 2, 1, 1, 1, 0, 0)) throw new AssertionError("valid cue refused");
      if (render() <= 0) throw new AssertionError("valid cue was silent");
      if (counts(arm)[1] != 1) throw new AssertionError("bad cue not counted");
      if (!op(arm, 3, 0, 0, 1, 1, 1, 0, 0)) throw new AssertionError("stop all refused");
      render();
      if (!op(arm, 1, 1, 2, 3601, 1, 1, 0, 0)) throw new AssertionError("play refused");
      render();
      if (counts(arm)[0] != 1) throw new AssertionError("future voice not live");
      if (!op(arm, 2, 1, 2, 3601, 1, 1, 0, 0)) throw new AssertionError("stop refused");
      render();
      if (counts(arm)[0] != 0 || counts(arm)[7] != 0) throw new AssertionError("cancelled voice still outstanding");
    } finally { close(arm); }
    arm = open(new String[]{args[1], args[2], args[3]}, 1, 2, decoded);
    try {
      if (counts(arm)[0] != 0 || counts(arm)[7] != 0) throw new AssertionError("reopen kept old voices or ops");
      if (!op(arm, 1, 101, 2, 1, 1, 1, 0, 0)) throw new AssertionError("reopened play refused");
      if (render() <= 0) throw new AssertionError("reopened stream was silent");
    } finally { close(arm); }
  }
}
`);
      const compiled = spawnSync(resolve(java.JAVA_HOME, 'bin/javac'), ['-d', dir, entry], { encoding: 'utf8' });
      expect(compiled.status).toBe(0);
      const wav = readFileSync(resolve(import.meta.dir, '../../web-js/conformance/sounds/assets/kick.wav'));
      const files = ['plain.wav', '🔔.wav'].map(name => resolve(dir, name));
      files.forEach(file => writeFileSync(file, wav));
      files.splice(1, 0, resolve(dir, 'broken.wav'));
      writeFileSync(files[1], 'not a WAV');
      const run = spawnSync(resolve(java.JAVA_HOME, 'bin/java'), ['--enable-native-access=ALL-UNNAMED', '-Xcheck:jni', '-cp', dir, 'dev.exact.canvas.SoundNative', library, ...files], { encoding: 'utf8' });
      expect({ status: run.status, error: run.stderr }).toEqual({ status: 0, error: '' });
      expect(run.stdout).toBe('');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 10000);
