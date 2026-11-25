// The JS target's decision to scroll to a fragment (host/web-js/router.js; LLP 1153 D5): a navigation counts only when a
// commit that lands assigns its very value to the router, and the scroll waits for every owned request in flight, a
// mutation's too, as the runner's `fragment_command` waits on `has_pending`. rt.js and router.js run beside stand-ins for the
// modules they import, and a fragment.js that records what it is asked.
import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { stageRuntime } from './stand-ins.mjs';

const dir = mkdtempSync(resolve(tmpdir(), 'exact-js-fragment-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
const REAL = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'request-send.js', 'shape.js', 'held.js', 'font-family.js', 'observers.js', 'grid.js', 'overlay.js', 'roster.js', 'router.js', 'route-pattern.js', 'schedule.js', 'budget.js', 'elements.js'];
stageRuntime(dir, REAL);
writeFileSync(resolve(dir, 'fragment.js'), `const asked = globalThis.fragmentAsked;
export const arm = (root, op) => asked.push(['arm', op.fragment, op.activation]);
export const scroll = (root, visit, fragment, log, activation) => asked.push(['scroll', fragment, activation]);
export const cancel = () => asked.push(['cancel']);`);

const turn = () => new Promise(r => setTimeout(r, 0));
const until = async ok => { for (let i = 0; i < 500 && !ok(); i++) await turn(); };

test('a fragment scrolls for a navigation a landed commit assigns the router, once its requests are answered', async () => {
  const page = globalThis.document;
  globalThis.document = { addEventListener() {}, getElementById: () => null, querySelector: () => null };
  const asked = globalThis.fragmentAsked = [], applied = [];
  try {
    const rt = await import(resolve(dir, 'rt.js'));
    await import(resolve(dir, 'fragment.js')); // loaded, so router.js's own import of it answers in a turn
    rt.routes([['home', '/', null, true, false], ['item', '/item/:id', 0, false, false]]);
    const nav = rt.sig(rt.launch('/item/7#moves'), null, 'nav');
    rt.router(nav, { apply: op => applied.push(op), project() {}, holdLink() { return null; }, personScrolls: 0 });
    await until(() => asked.length >= 2);
    expect(asked).toEqual([['arm', 'moves', 1], ['scroll', 'moves', 1]]); // the launch's
    const moves = () => rt.x_go(nav(), '/item/7#moves');

    // A navigation to the fragment shown, in a commit that is then refused, or computed and not assigned: nothing.
    expect(rt.commit(() => { rt.W(nav, moves()); throw new rt.Refusal('the save failed'); }, 'save')).toBe(false);
    rt.commit(() => { moves(); }, 'peek');
    await turn();
    expect([asked.length, applied.length]).toEqual([2, 1]);

    // Assigned, it is a change of its own to the history, and scrolls once the mutation it sent is answered.
    let answer;
    rt.data.answer = source => source === 'save' ? { promise: new Promise(r => { answer = r; }) } : null;
    const save = rt.mut('save', rt.sig(null, null, 'saved'), [], null);
    rt.commit(() => { rt.W(nav, moves()); rt.M(save, 'save', []); }, 'save');
    await turn();
    expect(applied.at(-1)).toMatchObject({ fragment: 'moves', scroll: true });
    expect(asked.slice(2)).toEqual([['arm', 'moves', 2]]);
    answer('ok');
    await until(() => asked.length >= 4);
    expect(asked.slice(2)).toEqual([['arm', 'moves', 2], ['scroll', 'moves', 2]]);

    // The app's own scrollIntoView ends a fragment's scroll held or yet to come.
    (globalThis.exact ??= {}).fragment = { cancel: (pending = true) => asked.push(['cancel', pending]) };
    rt.Hosts.scrollIntoView('row-1');
    expect(asked.at(-1)).toEqual(['cancel', true]);
    rt.Hosts.scrollBy('port', 0, 20);
    expect(asked.at(-1)).toEqual(['cancel', false]);

    // A scrollBy while data is pending leaves its activation armed.
    asked.length = 0;
    rt.commit(() => { rt.W(nav, moves()); rt.M(save, 'save', []); });
    await turn();
    rt.Hosts.scrollBy('port', 0, 20);
    expect(asked).toEqual([['arm', 'moves', 3], ['cancel', false]]);
    answer('ok');
    await until(() => asked.length >= 3);
    expect(asked.at(-1)).toEqual(['scroll', 'moves', 3]);

    // Replacing a resource's ordinary promise must not leave the old ticket holding the fragment.
    const choice = rt.sig(0), replies = [];
    rt.data.answer = (source, args) => source === 'detail' && args[0] === 1 ? { v: 'ready' }
      : { promise: new Promise(resolve => replies.push(resolve)) };
    let detail, region;
    rt.commit(() => {
      region = rt.scope(() => {
        detail = rt.res('detail', 'detail', () => [choice()], undefined, undefined, null, 'loading');
        rt.effect(() => detail());
      });
    });
    expect(detail.p()).toBe(true);
    asked.length = 0;
    rt.commit(() => { rt.W(nav, moves()); rt.W(choice, 1); });
    await turn();
    expect(detail()).toBe('ready');
    expect(asked).toEqual([['arm', 'moves', 4], ['scroll', 'moves', 4]]);

    // Leaving the resource's region retires its ticket even though the promise is still out.
    rt.commit(() => rt.W(choice, 2));
    expect(detail.p()).toBe(true);
    asked.length = 0;
    rt.commit(() => { rt.W(nav, moves()); rt.end(region); });
    await turn();
    expect(asked).toEqual([['arm', 'moves', 5], ['scroll', 'moves', 5]]);

    // A newest-winning mutation's old promise is obsolete; its replacement still holds the scroll.
    asked.length = 0;
    rt.commit(() => { rt.W(nav, moves()); rt.M(save, 'save', []); });
    rt.commit(() => rt.M(save, 'save', []));
    await turn();
    expect(asked).toEqual([['arm', 'moves', 6]]);
    replies.at(-1)('saved');
    await turn();
    expect(asked).toEqual([['arm', 'moves', 6], ['scroll', 'moves', 6]]);
    // Late obsolete replies neither scroll again nor replace the current value.
    for (const reply of replies) reply('obsolete');
    await turn();
    expect(detail()).toBe('ready');
    expect(asked).toHaveLength(2);

    // Every parallel send remains owned until it finishes, including one older than the latest send.
    const parallel = rt.mut('parallel', rt.sig(null), [], null, 2);
    asked.length = 0;
    rt.commit(() => { rt.W(nav, moves()); rt.M(parallel, 'save', []); rt.M(parallel, 'save', []); });
    await turn();
    expect(asked).toEqual([['arm', 'moves', 7]]);
    replies.at(-1)('newer');
    await turn();
    expect(asked).toHaveLength(1);
    replies.at(-2)('older');
    await turn();
    expect(asked).toEqual([['arm', 'moves', 7], ['scroll', 'moves', 7]]);

    asked.length = 0;
    rt.commit(() => { rt.W(nav, moves()); rt.M(save, 'save', []); });
    await turn();
    rt.commit(() => rt.W(nav, rt.x_go(nav(), '/')));
    await turn();
    expect(applied.at(-1).scroll).toBe(false);
    expect(asked).toEqual([['arm', 'moves', 8], ['cancel']]);
    replies.at(-1)('saved');
    await turn();
    expect(asked).toHaveLength(2);

    // A source not ready yet stops holding settlement when its region leaves too.
    rt.data.answer = () => null;
    asked.length = 0;
    rt.commit(() => {
      region = rt.scope(() => {
        const waiting = rt.res('waiting', 'unready', () => [], undefined, undefined, null, 'loading');
        rt.effect(() => waiting());
      });
      rt.W(nav, moves());
    });
    await turn();
    expect(asked).toEqual([['arm', 'moves', 9]]);
    rt.commit(() => rt.end(region));
    await turn();
    expect(asked).toEqual([['arm', 'moves', 9], ['scroll', 'moves', 9]]);
  } finally { if (page === undefined) delete globalThis.document; else globalThis.document = page; }
});

// The route corpus's launches (route/tests/corpus.json), as exact-route spends their ids: a query on a root location
// spends none, so a document rendered at `/?agent=1` is one the runtime at `/` adopts.
test('a launch spends the ids the route corpus says', async () => {
  const rt = await import(resolve(dir, 'rt.js'));
  const corpus = JSON.parse(readFileSync(new URL('../../../route/tests/corpus.json', import.meta.url), 'utf8'));
  const launches = corpus.sequences.flatMap(s => s.steps.filter(st => st.verb === 'launch').map(st => [s.table, st]));
  expect(launches.some(([, st]) => st.arg.startsWith('/?'))).toBe(true);
  for (const [table, { arg, expected }] of launches) {
    rt.routes(corpus.tables[table].routes.map(r => [r.name, r.pattern, r.parent, r.tab, r.notfound]));
    const [tab, tabs, next] = rt.launch(arg);
    const visits = tabs.map(([name, stack]) => ({ name, stack: stack.map(([id, name, url, tab]) => ({ id, name, url, tab })) }));
    const want = expected.tabs.map(({ name, stack }) => ({ name, stack: stack.map(({ id, name, url, tab }) => ({ id, name, url, tab })) }));
    expect({ arg, tab, visits, next }).toEqual({ arg, tab: expected.tab, visits: want, next: expected.next });
  }
});
