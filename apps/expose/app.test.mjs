import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';
import schema from '../../kernel/tables/schema.json';

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
    const session = await open({ host: 'web', app: 'expose', failFetch: 'https://api.open-meteo.com' });
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
  const thread = () => answer('conversation', ['expose'], store);
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
  } finally {
    globalThis.fetch = fetch;
    if (native === undefined) delete globalThis.__exact_message;
    else globalThis.__exact_message = native;
  }
});
