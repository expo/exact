import { expect, test } from 'bun:test';
import vm from 'node:vm';
import { declaration, fixture } from './glue-fixture.mjs';

test.each(['destroy', 'reload', 'editor replacement'])('beforeunload listeners end on %s', async mode => {
  const f = fixture(false);
  const window = new EventTarget();
  const listeners = new Set();
  const heard = [];
  Object.assign(f, {
    AbortController,
    log: message => {
      throw new Error(message);
    },
    addEventListener(type, fn, options) {
      listeners.add(fn);
      options?.signal?.addEventListener('abort', () => listeners.delete(fn), { once: true });
      window.addEventListener(type, fn, options);
    },
    removeEventListener(type, fn) {
      listeners.delete(fn);
      window.removeEventListener(type, fn);
    },
    inputHandlers: { mouse: (el, kind, event, fire) => fire(36, ''), arrive() {} },
    keyEvent: null, prepareContexts() {}, listSelection: null, pendingStore: { flush() {} },
    focusAutofocus() {}, positionContexts() {}, markScrollDocument() {}, refreshSymbols() {}, syncLists() {},
    forgetList() {}, followScroll() {}, sheetLeaves: () => false, dropSurface() {},
    markupPending: new WeakSet(), composing: new WeakSet(),
  });
  f.pieces.canvas2d = () => {};
  f.arrange.destroy = f.motion.destroy = () => {};
  f.navigation.project = () => {};
  f.collections.restyled = () => {};
  f.wasm.exact_dispatch = id => { heard.push(id); };
  vm.runInContext(['attach', 'apply', 'syncMarkup'].map(declaration).join('\n'), f);
  const box = () => ({ localName: 'div', dataset: {}, isConnected: true, addEventListener() {}, remove() {} });
  const mount = async id => {
    const el = mode === 'editor replacement' ? Object.assign(new f.HTMLTextAreaElement(), box(), {
      localName: 'textarea', hasAttribute: () => false, getAttribute: name => name === 'markup' ? 'markdown' : null,
    }) : box();
    f.views.set(id, el);
    f.attach(el, id, ['beforeunload']);
    if (mode === 'editor replacement') {
      const installed = new Promise(resolve => {
        f.loadAfterPaint = async () => (el, { replace }) => {
          replace(box());
          resolve();
        };
      });
      f.markupModule = null;
      f.syncMarkup(el);
      await installed;
    }
  };
  const end = id => mode === 'reload' ? f.bootNow(new Uint8Array([1]), null, () => true, null, true)
    : f.apply({ ops: [{ op: 'destroy', id }] });
  f.views.clear();
  for (let id = 1; id <= 40; id++) {
    await mount(id);
    await end(id);
  }
  await mount(41);
  window.dispatchEvent(new Event('beforeunload'));
  expect(heard).toEqual([41]);
  expect(listeners.size).toBe(1);
  await end(41);
  expect(listeners.size).toBe(0);
});

// A pan's handler is retired with its view, at a `destroy` and at a reload, so a contact it holds goes with it.
test.each(['destroy', 'reload'])('a pan retires with its view on %s', async mode => {
  const f = fixture(false);
  const retired = [];
  Object.assign(f, {
    AbortController,
    log: message => {
      throw new Error(message);
    },
    addEventListener() {},
    removeEventListener() {},
    inputHandlers: { pan: (el, id) => Object.assign(() => {}, { retire: () => retired.push(id) }), arrive() {} },
    keyEvent: null, prepareContexts() {}, listSelection: null, pendingStore: { flush() {} },
    focusAutofocus() {}, positionContexts() {}, markScrollDocument() {}, refreshSymbols() {}, syncLists() {},
    forgetList() {}, followScroll() {}, sheetLeaves: () => false, dropSurface() {},
    markupPending: new WeakSet(), composing: new WeakSet(),
  });
  f.pieces.canvas2d = () => {};
  f.arrange.destroy = f.motion.destroy = () => {};
  f.navigation.project = () => {};
  f.collections.restyled = () => {};
  vm.runInContext(['attach', 'apply'].map(declaration).join('\n'), f);
  const heard = {};
  const el = { localName: 'div', dataset: {}, isConnected: true, addEventListener(type, fn) { heard[type] = fn; }, remove() {} };
  f.views.clear();
  f.views.set(7, el);
  f.attach(el, 7, ['pan']);
  heard.pointerdown({});
  if (mode === 'reload') await f.bootNow(new Uint8Array([1]), null, () => true, null, true);
  else f.apply({ ops: [{ op: 'destroy', id: 7 }] });
  // The destroy's pre-pass and its op both run the view's teardown hooks; retiring twice is retiring.
  expect([...new Set(retired)]).toEqual([7]);
});
