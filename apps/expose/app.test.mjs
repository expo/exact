import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';
import schema from '../../kernel/tables/schema.json';

test('routes restore every screen from its URL', async () => {
  for (const [url, screen] of [
    ['/', 'lock-screen'], ['/home', 'home-screen'], ['/messages', 'inbox-screen'],
    ['/messages/nico', 'thread-screen'], ['/reader', 'reader-screen'],
    ['/reader/loud-clock', 'article-screen'], ['/settings', 'settings-screen'],
    ['/settings/intelligence', 'page-intelligence'], ['/settings/display', 'page-display'],
    ['/settings/about', 'page-about'],
  ]) {
    const session = await open({ host: 'web', app: 'expose', url,
      failFetch: 'https://' });
    try {
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(url);
      expect((await session.find(screen)).inactive).not.toBe(true);
      await session.carrier.reset({ keep: true, failFetch: 'https://' });
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(url);
      expect((await session.find(screen)).inactive).not.toBe(true);
    } finally {
      await session.close();
    }
  }
}, 30_000);

test('covered conversations keep their own names and transcripts', async () => {
  const session = await open({ host: 'web', app: 'expose', url: '/messages/expose', failFetch: 'https://' });
  const threads = () => session.carrier.evaluate(`Array.from(document.querySelectorAll('[data-testid="thread-screen"]'), el => el.textContent)`);
  try {
    await session.clock('settle');
    const before = await threads();
    expect(before[0]).toContain('Expose');
    await session.tap('thread-add-key');
    await session.clock('settle');
    expect(await threads()).toEqual(before);
    expect(await session.carrier.evaluate(`document.querySelector('[data-testid="page-intelligence"] [data-testid="back-settings"]').textContent`)).toContain('Messages');
    await session.tap('back-settings');
    await session.type('expose', '/messages/nico');
    await session.clock('settle');
    const covered = await threads();
    expect(covered).toHaveLength(2);
    expect(covered[0]).toBe(before[0]);
    expect(covered[1]).toContain('Nico');
    expect(covered[1]).not.toBe(covered[0]);
  } finally {
    await session.close();
  }
});

for (const [url, button, back, parent] of [
  ['/', 'notice-reader', 'back-reader', '/reader'],
  ['/home', 'widget-reader', 'back-reader', '/reader'],
  ['/', 'notice-update', 'back-settings', '/settings'],
  ['/home', 'app-typetour', 'back-settings', '/settings'],
]) {
  test(`${button} opens its declared parent from ${url}`, async () => {
    const session = await open({ host: 'web', app: 'expose', url, failFetch: 'https://' });
    try {
      await session.clock('settle');
      await session.tap(button);
      await session.tap(back);
      expect((await session.state()).navigation.url).toBe(parent);
    } finally {
      await session.close();
    }
  });
}

for (const [name, url, first, second, screen, restored, back] of [
  ['settings pages', '/settings', 'row-about', 'row-about-model', 'page-about', '/settings/about', 'back-settings'],
  ['Ask Expose', '/reader', 'article-loud-clock', 'ask-expose', 'article-screen', '/reader/loud-clock', 'back-reader'],
  ['conversation settings', '/messages', 'thread-expose', 'thread-add-key', 'thread-screen', '/messages/expose', 'thread-back'],
  ['apps', '/home', 'app-settings', 'row-app-messages', 'settings-screen', '/settings', 'home-bar'],
]) {
  test(`browser Back followed by the app Back returns through ${name}`, async () => {
    const session = await open({ host: 'web', app: 'expose', url, failFetch: 'https://' });
    try {
      await session.tap(first);
      const visit = (await session.find(screen)).id;
      await session.tap(second);
      if (name === 'settings pages')
        expect(await session.carrier.evaluate(`document.querySelector('[data-testid="page-intelligence"] [data-testid="back-settings"]').textContent`)).toContain('About');
      await session.tap('expose', { history: -1 });
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(restored);
      expect((await session.find(screen)).id).toBe(visit);
      await session.tap(back);
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(url);
    } finally {
      await session.close();
    }
  }, 30_000);
}

test('a Back control names the visit it returns to', async () => {
  for (const [from, url, screen, back] of [
    ['/messages', '/settings/display', 'page-display', 'back-settings'],
    ['/settings', '/reader/loud-clock', 'article-screen', 'back-reader'],
    ['/settings/about', '/settings/display', 'page-display', 'back-settings'],
  ]) {
    const session = await open({ host: 'web', app: 'expose', url: from, failFetch: 'https://' });
    try {
      await session.clock('settle');
      await session.type('expose', url);
      await session.clock('settle');
      const label = await session.carrier.evaluate(`Array.from(document.querySelectorAll('[data-testid="${screen}"] [data-testid="${back}"]')).at(-1).textContent`);
      expect(label).toContain({ '/messages': 'Messages', '/settings': 'Settings', '/settings/about': 'About' }[from]);
      await session.tap(back);
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(from);
    } finally {
      await session.close();
    }
  }
}, 30_000);

test('Send keeps focus in the conversation on top', async () => {
  const session = await open({ host: 'web', app: 'expose', url: '/messages/expose', failFetch: 'https://' });
  try {
    await session.clock('settle');
    await session.tap('thread-add-key');
    await session.tap('intelligence-chat');
    await session.clock('settle');
    await session.type('composer', 'Hello');
    await session.tap('send');
    await session.clock('settle');
    const focused = await session.carrier.evaluate(`(() => {
      const threads = Array.from(document.querySelectorAll('[data-testid="thread-screen"]'));
      return threads.length === 2 && threads[1].contains(document.activeElement)
        && document.activeElement.getAttribute('data-testid') === 'composer';
    })()`);
    expect(focused).toBe(true);
  } finally {
    await session.close();
  }
});

test('Reader retains its caller when an article extends a different chain', async () => {
  const session = await open({ host: 'web', app: 'expose', url: '/settings', failFetch: 'https://' });
  try {
    await session.tap('row-app-reader');
    const reader = (await session.find('reader-screen')).id;
    await session.tap('article-loud-clock');
    await session.tap('expose', { history: -1 });
    expect((await session.state()).navigation.url).toBe('/reader');
    expect((await session.find('reader-screen')).id).toBe(reader);
  } finally {
    await session.close();
  }
});

test('browser history visits settings, articles and conversations', async () => {
  const session = await open({ host: 'web', app: 'expose', failFetch: 'https://' });
  const at = async (url, screen) => {
    await session.clock('settle');
    expect((await session.state()).navigation.url).toBe(url);
    expect((await session.find(screen)).inactive).not.toBe(true);
  };
  try {
    await session.tap('lock-unlock');
    await session.tap('app-settings');
    for (const page of ['display', 'intelligence', 'about']) {
      await session.tap(`row-${page}`);
      await at(`/settings/${page}`, `page-${page}`);
      await session.tap('expose', { history: -1 });
      await at('/settings', 'settings-screen');
      await session.tap('expose', { history: 1 });
      await at(`/settings/${page}`, `page-${page}`);
      await session.tap('back-settings');
      await at('/settings', 'settings-screen');
    }
    await session.tap('home-bar');
    await session.tap('app-reader');
    await session.tap('article-loud-clock');
    await at('/reader/loud-clock', 'article-screen');
    await session.tap('expose', { history: -1 });
    await at('/reader', 'reader-screen');
    await session.tap('expose', { history: 1 });
    await at('/reader/loud-clock', 'article-screen');
    await session.tap('back-reader');
    await at('/reader', 'reader-screen');
    for (const id of ['grayscale', 'lilt', 'nine-days', 'assistant']) {
      await session.tap(`article-${id}`);
      await at(`/reader/${id}`, 'article-screen');
      await session.tap('back-reader');
      await at('/reader', 'reader-screen');
    }
    await session.tap('article-loud-clock');
    await session.tap('ask-expose');
    await at('/messages/expose', 'thread-screen');
    await session.tap('thread-add-key');
    await at('/settings/intelligence', 'page-intelligence');
    await session.tap('home-bar');
    await session.tap('app-messages');
    for (const id of ['expose', 'nico', 'crew', 'mom', 'builds']) {
      await session.tap(`thread-${id}`);
      await at(`/messages/${id}`, 'thread-screen');
      await session.type('composer', 'A draft');
      await session.tap('thread-back');
      await at('/messages', 'inbox-screen');
      expect((await session.state()).slots.draft).toBe('');
      await session.tap('expose', { history: 1 });
      await at(`/messages/${id}`, 'thread-screen');
      await session.tap('thread-back');
      await at('/messages', 'inbox-screen');
    }
    await session.tap('status-time');
    await at('/', 'lock-screen');
    await session.tap('notice-nico');
    await at('/messages/nico', 'thread-screen');
  } finally {
    await session.close();
  }
}, 30_000);

test('signal fills have readable Apple text and marks in both schemes', () => {
  const source = readFileSync(new URL('./app.contract', import.meta.url), 'utf8');
  const parts = readFileSync(new URL('./parts.contract', import.meta.url), 'utf8');
  const inks = [
    parts.match(/text badge.toString\(\).*?color="([^"]+)"/)?.[1],
    source.match(/text "Add an OpenRouter key".*?color="([^"]+)"/)?.[1],
    source.match(/path d="M12 19V6M6 12l6-6 6 6".*?stroke="([^"]+)"/)?.[1],
    source.match(/text "Save".*?color=\(keyDraft.trim\(\) == "" \? "disabledInk" : "([^"]+)"/)?.[1],
  ];
  const accent = source.match(/accent-color="([^"]+)"/)?.[1], failures = [];
  for (const [scheme, column] of [['light', 4], ['dark', 5]]) {
    const colour = value => {
      expect(value).toBeDefined();
      if (value === 'AccentColor') return colour(accent);
      const pair = value.match(/^light-dark\(([^,]+),\s*([^)]+)\)$/);
      if (pair) return colour(pair[scheme === 'light' ? 1 : 2].trim());
      if (!value.startsWith('#')) return colour(source.match(new RegExp(`^  ${value}="([^"]+)"`, 'm'))?.[1]
        ?? schema.colors.find(row => row[0] === value)?.[column]);
      expect(value).toMatch(/^#[0-9a-f]{6}(?:ff)?$/i);
      return [1, 3, 5].map(offset => parseInt(value.slice(offset, offset + 2), 16));
    };
    for (const [i, ink] of inks.entries()) {
      const contrast = ratio(colour(ink), colour('signal'));
      if (contrast < 4.5) failures.push({ control: ['badge', 'add key', 'send', 'save'][i], scheme, contrast });
    }
  }
  expect(failures).toEqual([]);
});

// Build first with `bun host/web/build.mjs expose`.
for (const appearance of ['system', 'light', 'dark']) {
  test(`Expose text remains readable with ${appearance} appearance`, async () => {
    const session = await open({ host: 'web', app: 'expose', failFetch: 'https://' });
    try {
      await session.clock('+1000');
      await session.clock('data');
      await session.tap('lock-unlock');
      await session.tap('app-settings');
      await session.tap('row-display');
      const selection = await session.type('appearance', appearance);
      expect(selection.error).toBeUndefined();
      const failures = [];
      const check = async (screen) => {
        await session.tree(screen);
        const result = await checkOp(session, [appearance === 'system' ? 'both' : appearance]);
        expect(result.check.map(report => report.scheme))
          .toEqual(appearance === 'system' ? ['light', 'dark'] : [appearance]);
        for (const report of result.check) {
          failures.push(...report.findings.filter(f => f.kind === 'contrast')
            .map(f => ({ screen, scheme: report.scheme, ...f })));
        }
      };
      await session.tap('status-time');
      await check('lock-screen');
      await session.tap('lock-unlock');
      await check('home-screen');
      await session.tap('app-messages');
      await check('inbox-screen');
      await session.tap('thread-nico');
      await check('thread-screen');
      await session.tap('thread-back');
      await session.tap('thread-expose');
      await session.type('composer', 'A draft');
      await check('thread-screen');
      await session.tap('thread-add-key');
      expect((await session.find('save-key')).props.disabled).toBe(true);
      await check('page-intelligence');
      await session.type('key', 'contrast-fixture');
      expect((await session.find('save-key')).props.disabled).not.toBe(true);
      await check('page-intelligence');
      await session.tap('home-bar');
      await session.tap('app-reader');
      await check('reader-screen');
      await session.tap('article-loud-clock');
      await check('article-screen');
      await session.tap('home-bar');
      await session.tap('app-settings');
      await check('settings-screen');
      expect(failures).toEqual([]);
    } finally {
      await session.close();
    }
  }, 30000);
}

// The module alone, over a stand-in fetch: each answer from OpenRouter that
// fails after its headers ends the turn, and Clear ends one still answering.
test('a body that fails after its headers ends the turn, and Clear aborts one still answering', async () => {
  const { answer } = await import('./app.ts');
  const store = { get: name => (name === 'assistant.key' ? 'sk-test' : null) };
  const thread = () => answer('conversations', [['expose'], 0, 0], store)[0];
  const fetch = globalThis.fetch, native = globalThis.__exact_message;
  try {
    // The native path reads one JSON body: a 200 that is not JSON.
    globalThis.__exact_message = () => {};
    globalThis.fetch = async () => new Response('<html>busy</html>', { status: 200 });
    let done = await answer('ask', ['expose', 'hello', '', 0], store);
    expect(done).toMatchObject({ done: true });
    expect(done.error).not.toBe('');
    expect(thread().streaming).toBe(false);

    // The page path reads the stream: one that errors after the headers.
    delete globalThis.__exact_message;
    globalThis.fetch = async () => new Response(new ReadableStream({
      start(c) { c.enqueue(new TextEncoder().encode('data: {"choices":[{"delta":{"content":"Hi"}}]}\n')); },
      pull(c) { c.error(new Error('connection reset')); },
    }), { status: 200 });
    done = await answer('ask', ['expose', 'again', '', 0], store);
    expect(done.error).toContain('connection reset');
    expect(thread().streaming).toBe(false);

    // A stream that never ends: Clear aborts it and empties the thread.
    globalThis.fetch = (_url, init) => new Promise((resolve, reject) => {
      init.signal.addEventListener('abort', () => reject(init.signal.reason));
    });
    const asked = answer('ask', ['expose', 'and again', '', 0], store);
    expect(thread().streaming).toBe(true);
    answer('clearThread', ['expose'], store);
    expect(thread()).toMatchObject({ streaming: false, error: '' });
    expect(thread().messages.length).toBe(1);
    expect(await asked).toMatchObject({ done: true, error: '' });
    expect(thread().messages.length).toBe(1);

    // A turn asked while one is answering is refused, and the thread says so
    // until the answer ends.
    let end;
    globalThis.fetch = () => new Promise(resolve => { end = () => resolve(new Response('', { status: 500 })); });
    const first = answer('ask', ['expose', 'one', '', 0], store);
    const refused = answer('ask', ['expose', '', 'a1', 0], store);
    expect(refused).toMatchObject({ done: false, error: 'Expose is still answering.' });
    expect(thread()).toMatchObject({ streaming: true, error: 'Expose is still answering.' });
    expect(thread().messages.map(m => m.body).slice(1)).toEqual(['one', '']);
    end();
    await first;
    expect(thread().error).not.toBe('Expose is still answering.');
  } finally {
    globalThis.fetch = fetch;
    if (native === undefined) delete globalThis.__exact_message;
    else globalThis.__exact_message = native;
  }
});
