import { expect, test } from 'bun:test';
import { uncachedCrates } from './new.mjs';

// A new game is made with no crate fetch only when both the game's graph error and the SDK lock's offline fetch
// say a crate is missing from Cargo's cache (expo/exact#446).
test('a new game defers its Cargo graph only for crates the cache lacks', () => {
  const graph = 'game Cargo graph: error: no matching package named `cfg-if` found';
  const missing = { ok: false, message: 'error: failed to download `glam v0.30.0`\nattempting to make an HTTP request, but --offline was specified' };
  expect(uncachedCrates(graph, () => missing)).toBe(true);
  expect(uncachedCrates('game Cargo graph: error: failed to download `accesskit v0.25.2` … --offline was specified', () => missing)).toBe(true);
  // The SDK's crates are all cached: the graph error is a real resolution failure.
  expect(uncachedCrates(graph, () => ({ ok: true, message: '' }))).toBe(false);
  // No Cargo, or a broken Cargo config, is not a missing crate.
  expect(uncachedCrates(graph, () => ({ ok: false, message: 'spawnSync cargo ENOENT' }))).toBe(false);
  // The game's own broken config fails its graph; the SDK's fetch, which does not read it, finds crates missing.
  expect(uncachedCrates('game Cargo graph: error: could not load Cargo configuration\n  invalid TOML', () => missing)).toBe(false);
  // Anything else is thrown, and asks no fetch.
  let asked = false;
  expect(uncachedCrates('Game already exists: /x', () => { asked = true; return missing; })).toBe(false);
  expect(uncachedCrates(undefined, () => { asked = true; return missing; })).toBe(false);
  expect(asked).toBe(false);
});
