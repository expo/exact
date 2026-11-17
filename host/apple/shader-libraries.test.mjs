// A GPU module's recorded MSL compiled at build time (shader-libraries.mjs).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { bundleShaderLibraries, languageFlag, recordedSources, shaderLibraries } from './shader-libraries.mjs';
import { metalVersion } from './svg-filter.mjs';

test('the language flag is one standard from MSL 3 and a platform\'s own before it', () => {
  assert.equal(languageFlag('4.0', 'macosx'), '-std=metal4.0');
  assert.equal(languageFlag('3.2', 'iphonesimulator'), '-std=metal3.2');
  assert.equal(languageFlag('2.4', 'macosx'), '-std=macos-metal2.4');
  assert.equal(languageFlag('2.4', 'iphoneos'), '-std=ios-metal2.4');
});

test.skipIf(process.platform !== 'darwin' || metalVersion('macosx').status !== 0)('recorded sources compile to libraries named by their keys; the rest compile at run time', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-shader-libraries-'));
  try {
    const app = { dir, name: 'fixture' };
    const sources = recordedSources(app);
    mkdirSync(sources, { recursive: true });
    const kernel = '#include <metal_stdlib>\nkernel void k(device float* x [[buffer(0)]]) { x[0] = 1; }\n';
    const options = '{"msl":"3.0","preserveInvariance":true}\n';
    writeFileSync(resolve(sources, '0123456789abcdef.metal'), kernel);
    writeFileSync(resolve(sources, '0123456789abcdef.json'), options);
    // No options: not compiled. Not MSL: refused, left to the run-time compile.
    writeFileSync(resolve(sources, '1111111111111111.metal'), kernel);
    writeFileSync(resolve(sources, '2222222222222222.metal'), 'not metal');
    writeFileSync(resolve(sources, '2222222222222222.json'), options);
    const out = resolve(dir, 'out');
    assert.equal(shaderLibraries(app, 'macosx', out), 1);
    assert.deepEqual(readdirSync(out), ['0123456789abcdef.metallib']);
    // A second build takes the kept library.
    rmSync(out, { recursive: true });
    assert.equal(shaderLibraries(app, 'macosx', out), 1);
    // An app that recorded nothing ships nothing.
    assert.equal(shaderLibraries({ dir: resolve(dir, 'none'), name: 'none' }, 'macosx', resolve(dir, 'empty')), 0);
    assert.ok(!existsSync(resolve(dir, 'empty')));
    // An app whose only GPU modules are named ones ships them too; one with
    // no GPU module ships none, and a stale tree goes.
    const capture = resolve(dir, 'capture');
    assert.equal(bundleShaderLibraries(app, 'macosx', capture, [{ built: 'libgame_gpu_world.dylib', framework: 'ExactGpuWorld' }]), 1);
    assert.deepEqual(readdirSync(resolve(capture, 'exact-metal')), ['0123456789abcdef.metallib']);
    assert.equal(bundleShaderLibraries(app, 'macosx', capture, []), 0);
    assert.ok(!existsSync(resolve(capture, 'exact-metal')));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
