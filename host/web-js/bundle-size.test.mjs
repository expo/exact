import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { need } from '../../scripts/coverage.mjs';
import { builtFile } from './content-names.mjs';

const dist = process.env.EXACT_REALWORLD_DIST;
const check = need('fixture', dist ? null : 'build RealWorld with --production and set EXACT_REALWORLD_DIST to its directory');

check('RealWorld production app.js omits the file-body path', () => {
  const js = readFileSync(resolve(dist, builtFile(dist, 'app.js')), 'utf8');
  assert.ok(!js.includes('exactBodyFrom must be an app:/ path'), 'file-body validation was linked into app.js');
});
