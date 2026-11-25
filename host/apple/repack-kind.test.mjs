// A repack's build kind (ExactBuildKind) is its base's full build's: `bun test host/apple/repack-kind.test.mjs`.
import { test, expect } from 'bun:test';
import { fileURLToPath } from 'node:url';
import { plistFile } from './assets.mjs';
import { buildInfo } from './buildinfo.mjs';
import { archivedBase } from './repack.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const stampOf = (options) => plistFile(buildInfo({ dir: root, manifest: {} }, { root, env: {}, ...options }));

test('a repack stamps the kind its base was stamped with, as bundle.mjs stamps a full build', () => {
  // bundle.mjs: an IPA is an archive only when distributed; a development archive is a debug build.
  const development = stampOf({ archive: false, production: false });
  expect(development).toContain('<string>debug</string>');
  expect(archivedBase(development)).toBe(false);
  expect(buildInfo({ dir: root, manifest: {} }, { root, env: {}, archive: archivedBase(development) }).ExactBuildKind).toBe('debug');
  // An SDK whose full build stamps every IPA as an archive gives its repacks the same kind.
  expect(archivedBase(stampOf({ archive: true }))).toBe(true);
  expect(archivedBase(stampOf({ archive: false, production: true }))).toBe(false);
  expect(archivedBase('')).toBe(false);
});
