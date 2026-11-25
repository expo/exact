// Issue #110: an aria-keyshortcuts chord whose key types no Latin character
// (Korean 2-Set's ㅠ on KeyB, Russian, Greek) is its physical key's (`code`),
// as web apps match; a Latin character is the key wherever it is (AZERTY,
// Dvorak, German's ö), so one chord fires, never two.
import { test, expect } from 'bun:test';
import { formatShortcut, resolveShortcuts } from './navigation/environment.js';
import { glueDeclarations, hostFixtureFile, inputFixtureSource } from './tests/glue-fixture.mjs';
import { Script } from 'node:vm';

const inputScript = new Script(`${await inputFixtureSource()}\nexact.createInputHandlers({ root, views: new Map(), retiredViews: new WeakSet(), ready: () => true, inertAncestor: () => false, dispatch() {} });`, { filename: 'input-glue.js' });
// `shown`: the buttons have boxes; `marked`: each is a Back control its route's bar stands for (navigation.js).
function page(chords, { shown = true, marked = false } = {}) {
  const clicks = [], listeners = [];
  const buttons = chords.map(chord => ({ isConnected: true, disabled: false, getClientRects: () => (shown ? [{}] : []),
    hasAttribute: name => marked && name === 'data-exact-bar-back', getAttribute: () => chord, click() { clicks.push(chord); } }));
  const querySelectorAll = selector => selector === 'button[aria-keyshortcuts]' ? buttons : [];
  const context = { exact: {},
    document: { querySelectorAll, addEventListener(kind, fn, capture) { if (kind === 'keydown' && capture) listeners.push(fn); }, activeElement: { closest: () => null },
      getElementById: id => (id === 'exact-snap' ? {} : null), styleSheets: [], documentElement: { dataset: {} } },
    getComputedStyle: () => ({ visibility: 'visible' }),
    // What the input glue reads as it starts that no chord here touches: its sheets' observers, hover, looks.
    MutationObserver: class { observe() {} disconnect() {} takeRecords() { return []; } },
    queueMicrotask, Element: class {}, matchMedia: () => ({ matches: false, addEventListener() {} }),
    addEventListener() {}, removeEventListener() {},
    root: { querySelectorAll, addEventListener() {}, contains: () => true },
  };
  inputScript.runInNewContext(context);
  const key = (key, code, extra = {}) => {
    const e = { key, code, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, composedPath: () => [],
      preventDefault() { this.prevented = true; }, stopImmediatePropagation() {}, ...extra };
    for (const fn of listeners) fn(e);
    return e.prevented === true;
  };
  return { clicks, key };
}

test('a non-Latin key matches the chord its physical key names', () => {
  const p = page(['Meta+B', 'Meta+A', 'Meta+1', 'Meta+/', 'Meta+Shift+B', 'Meta+Shift+#']);
  expect([p.key('ㅠ', 'KeyB'), p.key('ф', 'KeyA'), p.key('ㅂ', 'Digit1'), p.key('ㅠ', 'Slash'),
    p.key('ㅠ', 'KeyB', { shiftKey: true }), p.key('№', 'Digit3', { shiftKey: true })]).toEqual([true, true, true, true, true, true]);
  expect(p.clicks).toEqual(['Meta+B', 'Meta+A', 'Meta+1', 'Meta+/', 'Meta+Shift+B', 'Meta+Shift+#']);
});

test('a Latin key is its own chord, never its physical key\'s', () => {
  const p = page(['Meta+A', 'Meta+Q', 'Meta+B', 'Meta+;', 'Meta+2', 'Alt+c']);
  // AZERTY's q on KeyA, Dvorak's b on KeyN, German's ö on Semicolon, AZERTY's é on Digit2, Option's ç.
  expect([p.key('q', 'KeyA'), p.key('b', 'KeyN'), p.key('x', 'KeyB'), p.key('ö', 'Semicolon'), p.key('é', 'Digit2'),
    p.key('ç', 'KeyC', { metaKey: false, altKey: true })]).toEqual([true, true, false, false, false, false]);
  expect(p.clicks).toEqual(['Meta+Q', 'Meta+B']);
});

test('the fallback keeps the chord\'s modifiers and refuses keys it cannot name', () => {
  const p = page(['Meta+B', 'Control+B']);
  expect([p.key('ㅠ', 'KeyB', { shiftKey: true }), p.key('ㅠ', 'KeyB', { altKey: true }), p.key('ㅠ', 'KeyB', { metaKey: false }),
    p.key('ㅠ', 'IntlRo'), p.key('ㅠ', undefined), p.key(undefined, 'KeyB'), p.key('Process', 'KeyB')]).toEqual([false, false, false, false, false, false, false]);
  expect(p.key('ㅠ', 'KeyB', { metaKey: false, ctrlKey: true })).toBe(true);
  expect(p.clicks).toEqual(['Control+B']);
});

// LLP 1116.000 D4: a Back control the bar stands for is not shown, and its chord still presses it; a button that is
// not shown otherwise takes none.
test('a Back control the bar stands for keeps its chord', () => {
  const hidden = page(['Meta+['], { shown: false }), marked = page(['Meta+['], { shown: false, marked: true });
  expect([hidden.key('[', 'BracketLeft'), hidden.clicks, marked.key('[', 'BracketLeft'), marked.clicks]).toEqual([false, [], true, ['Meta+[']]);
});

test('the Wasm prop writer resolves a portable list when setting and updating it', () => {
  const source = hostFixtureFile(new URL('./wasm/views.js', import.meta.url), 'utf8');
  const write = new Script(glueDeclarations(source, ['applyProps']) + '\napplyProps');
  const element = { setAttribute(k, v) { this[k] = v; }, removeAttribute(k) { delete this[k]; } };
  const key = 'aria-keyshortcuts';
  for (const [nav, primary] of [[{ platform: 'iPad' }, 'Meta'], [{ userAgentData: { platform: 'Linux' } }, 'Control']]) {
    const apply = write.runInNewContext({ host: { inputReady: true }, HTMLIFrameElement: class {}, HTMLImageElement: class {}, HTMLMediaElement: class {},
      syncMedia() {}, syncMarkup() {}, resolveShortcuts: chords => resolveShortcuts(chords, nav) });
    apply(element, { [key]: 'Escape\u0085Mod+O' });
    expect(element[key]).toBe('Escape ' + primary + '+O');
    apply(element, { [key]: 'Control+Mod\uFEFFMod+P' });
    expect(element[key]).toBe(primary + '+P');
    apply(element, { [key]: 'Escape Mod+Hyper+K Mod+𐐀' });
    expect(element[key]).toBe('Escape');
    apply(element, { [key]: 'Control+Mod' });
    expect(element[key]).toBeUndefined();
  }
});

test('named keys, functions and modifier order use the reader’s notation', () => {
  for (const [chord, apple, windows] of [
    ['Meta+Shift+Alt+Control+ArrowUp', '⌃⌥⇧⌘↑', 'Windows logo key+Ctrl+Alt+Shift+Up arrow'],
    ['Mod+ß', '⌘ß', 'Ctrl+ß'], ['Mod+Insert', '⌘Insert', 'Ctrl+Insert'], ['Mod+𐐀', '', ''],
    ['Mod+Enter', '⌘↩', 'Ctrl+Enter'], ['Space', 'Space', 'Spacebar'], ['Escape', '⎋', 'Esc'],
    ['Mod++', '⌘+', 'Ctrl+Plus sign'], ['F35', 'F35', 'F35'], ['F36', '', ''],
    ['Mod+Meta+O', '', ''], ['Mod+Control+O', '', ''], ['Hyper+O', '', ''], ['Mod+O Mod+P', '', ''],
  ]) {
    expect(formatShortcut(chord, { platform: 'MacIntel' })).toBe(apple);
    expect(formatShortcut(chord, { platform: 'Win32' })).toBe(windows);
    expect(formatShortcut(chord, {})).toBe('');
  }
});
