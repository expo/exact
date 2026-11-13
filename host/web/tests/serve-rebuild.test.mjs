// serve.mjs builds an app again before a page when a source changed since the build (LLP 1153.001 D3). A build it
// ran covers the sources as they were when it started: a save while it ran is built too, though its receipt is newer.
import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { appSources, rebuildOnChange } from '../serve.mjs';

test('a save while a rebuild runs is built once more before the page is answered', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-serve-rebuild-'));
  try {
    const app = join(root, 'app'), dist = join(app, 'dist'), source = join(app, 'app.contract'), marker = join(dist, '.exact-build.json');
    mkdirSync(dist, { recursive: true });
    writeFileSync(join(app, 'app.json'), JSON.stringify({ app: { id: 'dev.exact.rebuild' } }));
    writeFileSync(source, 'one\n');
    const t = Date.now() / 1000 - 100;
    for (const file of [source, join(app, 'app.json')]) utimesSync(file, t, t);
    const receipt = (at) => {
      writeFileSync(marker, JSON.stringify({ app: { id: 'dev.exact.rebuild' }, release: true, target: 'js', sources: appSources(app, dist).digest }));
      utimesSync(marker, at, at);
    };
    receipt(t + 1);
    // The first build sees the second save land while it runs, and writes its receipt after it.
    const builds = [];
    const build = async () => {
      builds.push(builds.length);
      if (builds.length === 1) { writeFileSync(source, 'three\n'); utimesSync(source, t + 20, t + 20); receipt(t + 30); }
      else receipt(t + 40);
      return { code: 0, said: '' };
    };
    const rebuild = rebuildOnChange(dist, build);
    expect(await rebuild()).toBe(null);
    expect(builds.length).toBe(0);
    await new Promise((ok) => setTimeout(ok, 350));
    writeFileSync(source, 'two\n');
    utimesSync(source, t + 10, t + 10);
    expect(await rebuild()).toBe(null);
    expect(builds.length).toBe(2);
    // Nothing changed since: no build.
    await new Promise((ok) => setTimeout(ok, 350));
    expect(await rebuild()).toBe(null);
    expect(builds.length).toBe(2);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
