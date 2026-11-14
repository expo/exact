import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { bundledFamily, thinTextures, unbundledTextures } from './texture-families.mjs';

const baked = ['assets/fox/0.tex', 'assets/fox/0.bc.tex', 'assets/fox/0.astc.tex'];

test("a Mac's bundle holds a texture's BC payload, an iPhone's its ASTC payload, others all three", () => {
  const names = [...baked, 'assets/fox.model', 'deck/0.tex', 'assets/a.b.tex', 'assets/a.b.bc.tex', 'assets/a.b.astc.tex'];
  assert.deepEqual([...unbundledTextures(names, bundledFamily('macos'))].sort(), ['assets/a.b.astc.tex', 'assets/a.b.tex', 'assets/fox/0.astc.tex', 'assets/fox/0.tex']);
  assert.deepEqual([...unbundledTextures(names, bundledFamily('ios'))].sort(), ['assets/a.b.bc.tex', 'assets/a.b.tex', 'assets/fox/0.bc.tex', 'assets/fox/0.tex']);
  for (const platform of ['web', 'linux', 'windows', 'android']) assert.equal(unbundledTextures(names, bundledFamily(platform)).size, 0, platform);
});

test('a texture the bake made no payload for stays, and so does a payload with no RGBA8 beside it', () => {
  assert.equal(unbundledTextures(['assets/hand.tex'], 'bc').size, 0);
  assert.equal(unbundledTextures(['assets/a.astc.tex'], 'bc').size, 0);
  // A payload of the platform's own family alone takes its RGBA8 file's place.
  assert.deepEqual([...unbundledTextures(['assets/b.tex', 'assets/b.bc.tex'], 'bc')], ['assets/b.tex']);
  assert.equal(unbundledTextures(['assets/b.tex', 'assets/b.astc.tex'], 'bc').size, 1);
});

test("a bundle's copy of assets/ keeps one family of a baked texture", () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-thin-'));
  try {
    mkdirSync(join(dir, 'assets/fox'), { recursive: true });
    // A 1024² colour map's bytes, as the bake writes them (LLP 1046.003).
    const sizes = { 'fox/0.tex': 5_592_561, 'fox/0.bc.tex': 1_398_280, 'fox/0.astc.tex': 1_398_284, 'hand.tex': 1_495 };
    for (const [name, bytes] of Object.entries(sizes)) writeFileSync(join(dir, 'assets', name), Buffer.alloc(bytes));
    const held = () => readdirSync(join(dir, 'assets'), { recursive: true }).filter(n => n.endsWith('.tex')).sort();
    const bytes = () => held().reduce((sum, n) => sum + statSync(join(dir, 'assets', n)).size, 0);
    const before = bytes();
    thinTextures(dir, null);
    assert.equal(bytes(), before);
    thinTextures(dir, 'astc');
    assert.deepEqual(held(), ['fox/0.astc.tex', 'hand.tex']);
    assert.equal(before - bytes(), 5_592_561 + 1_398_280);
    assert.equal(bytes(), 1_398_284 + 1_495, "a sixth of the baked texture's three files");
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
