// LLP 1115.000 D4.6: a page without strings tables keeps the manifest's `lang`,
// which screen-reader voices, hyphenation and `:lang()` read: the shell the
// development build writes (host/web-js/build.mjs), and a page the JavaScript
// runtime renders over it (host/web-js/render.mjs). A real build of an app
// that declares a language, outside the repository (`EXACT_APP_DIR`), then a
// real render of its one page.
import { test, expect, afterAll } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { renderer } from '../../web-js/render.mjs';
import { fileURLToPath } from 'node:url';
import { standIns } from './stand-ins.mjs';

const ROOT = resolve(fileURLToPath(new URL('../../..', import.meta.url)));
const dir = mkdtempSync(resolve(tmpdir(), 'exact-js-lang-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

test('the JS build\'s shell and a rendered page keep the manifest\'s lang', async () => {
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Olá', lang: 'pt-BR', app: { id: 'com.example.ola', name: 'Olá' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Olá, mundo" testId="hello"\n');
  const dist = resolve(dir, 'dist');
  const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'ola', '--out', dist, '--render', 'js'], { cwd: ROOT, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir } });
  expect(build.status, build.stderr).toBe(0);
  expect(readFileSync(resolve(dist, 'index.html'), 'utf8')).toStartWith('<!doctype html>\n<html lang="pt-BR">\n');
  expect(JSON.parse(readFileSync(resolve(dist, 'manifest.json'), 'utf8')).lang).toBe('pt-BR');
  const page = await renderer(dist)('/');
  expect(page.html).toStartWith('<!doctype html>\n<html lang="pt-BR" dir="ltr">\n');
  expect(page.html).toContain('Olá, mundo');
}, 120000);

test('a chosen language commits atomically, persists, and initializes the next launch', async () => {
  const original = globalThis.localStorage;
  const values = new Map(), key = 'exact.state.#locale';
  let blocked = false;
  globalThis.localStorage = {
    getItem: k => values.get(k) ?? null,
    setItem(k, v) { if (blocked) throw new Error('storage blocked'); values.set(k, v); },
    removeItem(k) { if (blocked) throw new Error('storage blocked'); values.delete(k); },
  };
  let boots = 0;
  async function launch(chooses = true) {
    const here = resolve(dir, `runtime-${boots++}`);
    mkdirSync(here);
    const real = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'chosen.js', 'shape.js', 'overlay.js', 'roster.js', 'router.js', 'route-pattern.js', 'schedule.js', 'resource-inspection.js', 'held.js', 'uploads.js'];
    for (const file of real) copyFileSync(resolve(ROOT, 'host/web-js', file), resolve(here, file));
    for (const [file, source] of standIns(real)) writeFileSync(resolve(here, file), source);
    const nav = resolve(here, 'navigation.js');
    writeFileSync(nav, readFileSync(nav, 'utf8').replace('reportPlace = () => {}', 'reportPlace = () => "en-US\\0UTC\\0seed\\0en-US"'));
    const rt = await import(resolve(here, 'rt.js'));
    const choice = chooses ? await import(resolve(here, 'chosen.js')) : null;
    rt.strings([['en', false, { greeting: 'Hello' }], ['de', false, { greeting: 'Hallo' }], ['ar', true, { greeting: 'مرحبا' }]]);
    const slot = rt.sig(rt.locale(), 's');
    choice?.localeSlot(slot);
    const initial = rt.text(slot(), 'greeting', []).join('');
    rt.language(slot);
    rt.data.answer = () => ({ v: rt.resolvedLocale() });
    const time = rt.res('time', 'exactTime', () => [], '', [], 's', '');
    return { ...rt, ...choice, slot, initial, time };
  }
  try {
    const r = await launch();
    expect([r.initial, r.time(), r.resolvedLanguage()]).toEqual(['Hello', 'en', ['en', 'ltr']]);
    expect(r.commit(() => r.setLocale('de-DE'))).toBe(true);
    expect([r.slot(), r.time(), r.locale(), values.get(key)]).toEqual(['de', 'de', 'de', '"de-DE"']);
    expect((await launch()).initial).toBe('Hallo');
    expect((await launch(false)).initial).toBe('Hello');
    const rendering = globalThis.__exactRender;
    try {
      globalThis.__exactRender = true;
      expect((await launch()).initial).toBe('Hello');
    } finally {
      if (rendering === undefined) delete globalThis.__exactRender; else globalThis.__exactRender = rendering;
    }
    let refuseArabic = true;
    r.memo(() => { if (r.slot() === 'ar' && refuseArabic) throw new r.Refusal('stop'); return 0; })();
    expect(r.commit(() => r.setLocale('ar'))).toBe(false);
    expect([r.slot(), r.time(), r.locale(), values.get(key)]).toEqual(['de', 'de', 'de', '"de-DE"']);
    refuseArabic = false;
    expect(r.commit(() => { r.setLocale('en'); r.setLocale('ar'); })).toBe(true);
    expect([r.slot(), r.time(), r.resolvedLanguage(), values.get(key)]).toEqual(['ar', 'ar', ['ar', 'rtl'], '"ar"']);
    r.commit(() => r.setLocale('bad tag'));
    expect([r.slot(), values.get(key)]).toEqual(['ar', '"ar"']);
    expect(r.journal.some(line => line.includes('setLocale("bad tag") refused'))).toBe(true);
    blocked = true;
    r.commit(() => r.setLocale('de'));
    expect([r.slot(), values.get(key)]).toEqual(['de', '"ar"']);
    blocked = false;
    r.commit(() => { throw new r.Refusal('still refused'); });
    expect(values.get(key)).toBe('"ar"');
    r.commit(() => {});
    expect(values.get(key)).toBe('"de"');
    r.commit(() => r.setLocale(''));
    expect([r.slot(), r.time(), r.locale(), values.has(key)]).toEqual(['en', 'en', 'en', false]);
    expect((await launch()).initial).toBe('Hello');
  } finally {
    if (original === undefined) delete globalThis.localStorage; else globalThis.localStorage = original;
  }
});
