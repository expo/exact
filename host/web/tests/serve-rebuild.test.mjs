// serve.mjs builds an app again before a page when a source changed since the build (LLP 1153.001 D3). A build it
// ran covers the sources as they were when it started: a save while it ran is built too, though its receipt is newer.
import { test, expect } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { appSources, rebuildOnChange } from '../serve.mjs';
import { recordLinkedSources } from '../../../scripts/web-build.mjs';

test('a save while a rebuild runs is built once more before the page is answered', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-serve-rebuild-'));
  try {
    const app = join(root, 'app'), dist = join(app, 'dist'), source = join(app, 'app.contract'), marker = join(dist, '.exact-build.json');
    mkdirSync(dist, { recursive: true });
    writeFileSync(join(app, 'app.json'), JSON.stringify({ app: { id: 'dev.exact' }, env: ['BRAND'], variants: { red: { idSuffix: '.rebuild', env: { BRAND: 'red' } } } }));
    writeFileSync(source, 'one\n');
    const t = Date.now() / 1000 - 100;
    for (const file of [source, join(app, 'app.json')]) utimesSync(file, t, t);
    const receipt = (at) => {
      writeFileSync(marker, JSON.stringify({ app: { id: 'dev.exact.rebuild' }, release: true, target: 'js', linkedSources: null, env: { BRAND: { value: 'red', source: 'variant' } }, sources: appSources(app, dist).digest }));
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

test('the server rebuilds for the shared Contract package a site consumed', async () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'exact-serve-workspace-')));
  try {
    const app = join(root, 'app'), dist = join(app, 'dist'), generated = join(root, 'generated');
    const modules = join(app, 'node_modules'), link = join(modules, 'ui');
    const library = join(root, 'ui'), replacement = join(root, 'replacement');
    const marker = join(dist, '.exact-build.json'), entry = join(link, 'index.contract');
    const missing = join(link, 'ui.forma'), manifest = join(link, 'package.json');
    for (const dir of [dist, generated, modules, library, replacement]) mkdirSync(dir, { recursive: true });
    for (const dir of [library, replacement]) {
      writeFileSync(join(dir, 'index.contract'), 'one');
      writeFileSync(join(dir, 'package.json'), '{}');
    }
    symlinkSync(library, link, 'dir');
    writeFileSync(join(app, 'app.json'), JSON.stringify({ app: { id: 'dev.exact.shared' } }));
    const publish = () => {
      const sha256 = createHash('sha256').update(readFileSync(entry)).digest('hex');
      writeFileSync(join(generated, 'app.plan.map.json'), JSON.stringify({ sources: [{ file: realpathSync(entry), sha256 }] }));
      writeFileSync(join(generated, 'contract-lookups.json'), JSON.stringify([manifest, entry, missing]));
      const linkedSources = recordLinkedSources(app, dist, generated, Infinity);
      const card = { app: { id: 'dev.exact.shared' }, target: 'js', release: true, sources: appSources(app, dist).digest, linkedSources };
      writeFileSync(marker, JSON.stringify(card));
      expect(readFileSync(marker, 'utf8')).not.toContain(root);
    };
    let builds = 0;
    const check = async (during) => {
      const start = builds;
      await rebuildOnChange(dist, async () => {
        publish();
        if (++builds === start + 1) during?.();
        return { code: 0, said: '' };
      })();
      return builds - start;
    };
    publish();
    writeFileSync(join(library, 'unrelated.contract'), 'unused');
    expect(await check()).toBe(0);
    writeFileSync(entry, 'two');
    utimesSync(entry, 100, 100);
    expect(await check(() => writeFileSync(entry, 'three'))).toBe(2);
    expect(await check()).toBe(0);
    writeFileSync(join(replacement, 'index.contract'), 'three');
    rmSync(link);
    symlinkSync(replacement, link, 'dir');
    expect(await check()).toBe(1);
    writeFileSync(missing, 'present');
    expect(await check()).toBe(1);
    rmSync(missing);
    expect(await check()).toBe(1);
    const old = JSON.parse(readFileSync(marker));
    delete old.linkedSources;
    writeFileSync(marker, JSON.stringify(old));
    expect(await check()).toBe(1);
    expect(await check()).toBe(0);
    rmSync(entry);
    const failure = rebuildOnChange(dist, async () => ({ code: 1, said: 'unreadable package entry' }));
    expect(await failure()).toBe('unreadable package entry');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
