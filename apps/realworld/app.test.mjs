import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';
import { HOSTS, startNet } from '../../host/web-js/conformance/net.mjs';
import schema from '../../kernel/tables/schema.json';

test('authored accent fills have readable Apple ink in both schemes', () => {
  const source = readFileSync(new URL('./app.contract', import.meta.url), 'utf8');
  const nodes = [
    ['large button', /^style BtnLg\n(.*)$/m],
    ['feed favourite', /    button press=favorite\(slug,[\s\S]*?testId=`favorite-\$\{slug\}`/],
    ['article favourite', /          button press=favorite\(post.slug,[\s\S]*?testId="favorite"/],
    ['post comment', /                    button press=postComment[\s\S]*?testId="post-comment"/],
  ];
  const values = (node, property) => {
    const row = node.match(new RegExp(`(?:^|\\s)${property}=("[^"]+"|\\([^\\n]+\\))`));
    expect(row).not.toBeNull();
    return [...row[1].matchAll(/"([^"]+)"/g)].map(m => m[1]);
  };
  const accent = values(source, 'accent-color')[0], failures = [];
  for (const [scheme, column] of [['light', 4], ['dark', 5]]) {
    const colour = value => {
      if (value === 'AccentColor') return colour(accent);
      const pair = value.match(/^light-dark\(([^,]+),\s*([^)]+)\)$/);
      if (pair) return colour(pair[scheme === 'light' ? 1 : 2].trim());
      if (!value.startsWith('#')) {
        value = source.match(new RegExp(`^  ${value}="([^"]+)"`, 'm'))?.[1]
          ?? schema.colors.find(row => row[0] === value.replace(/^-exact-/, ''))?.[column];
        expect(value).toBeDefined();
        return colour(value);
      }
      expect(value).toMatch(/^#[0-9a-f]{6}(?:ff)?$/i);
      return [1, 3, 5].map(offset => parseInt(value.slice(offset, offset + 2), 16));
    };
    for (const [name, pattern] of nodes) {
      const node = source.match(pattern);
      expect(node).not.toBeNull();
      const inks = values(node[0], 'color'), fills = values(node[0], 'background-color');
      expect(inks.length).toBe(fills.length);
      for (let i = 0; i < inks.length; i++) {
        const contrast = ratio(colour(inks[i]), colour(fills[i]));
        if (contrast < (fills[i] === 'AccentColor' ? 4.5 : 3)) failures.push({ name, scheme, contrast });
      }
    }
  }
  expect(failures).toEqual([]);
});

// Build first with `bun host/web/build.mjs realworld`.
for (const scheme of ['light', 'dark']) {
  test(`filled controls remain readable in ${scheme} mode`, async () => {
    const favourites = new Set();
    const json = value => ({ status: 200, headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) });
    const net = await startNet(undefined, {
      'api.realworld.show': (method, url, headers, body) => {
        if (url.pathname === '/api/users/login' || url.pathname === '/api/user') {
          return json({ user: { username: 'reader', email: 'reader@example.invalid', bio: '', image: '', token: 'fixture' } });
        }
        const favourite = url.pathname.match(/^\/api\/articles\/([^/]+)\/favorite$/);
        if (favourite) {
          if (method === 'POST') favourites.add(favourite[1]); else favourites.delete(favourite[1]);
          url = new URL(`/api/articles/${favourite[1]}`, url);
          method = 'GET';
        }
        const reply = HOSTS['api.realworld.show'](method, url, headers, body);
        if (!reply?.headers['content-type']?.includes('json')) return reply;
        const data = JSON.parse(reply.body);
        for (const article of data.articles ?? (data.article ? [data.article] : [])) {
          article.favorited = favourites.has(article.slug);
          article.favoritesCount = article.favorited ? 5 : 4;
        }
        return { ...reply, body: JSON.stringify(data) };
      },
    });
    let session;
    try {
      session = await open({ host: 'web', app: 'realworld', env: net.env });
      await session.prefer({ 'prefers-color-scheme': scheme });
      const check = async () => {
        const result = await checkOp(session, [scheme]);
        expect(result.check.flatMap(r => r.findings).filter(f => f.kind === 'contrast')).toEqual([]);
      };
      await session.clock('data');
      await session.tap('nav-login');
      await check();
      await session.type('email', 'reader@example.invalid');
      await session.type('password', 'fixture');
      await session.tap('submit');
      await session.clock('data');
      expect((await session.state()).derives.current.name).toBe('home');
      await session.tap('favorite-how-to-learn-javascript-efficiently');
      await session.clock('data');
      expect(favourites.has('how-to-learn-javascript-efficiently')).toBe(true);
      await check();
      await session.tap('open-how-to-learn-javascript-efficiently');
      await session.clock('data');
      await check();
      const favourite = (await session.tree()).nodes.find(n => n.props?.testId === 'favorite');
      await session.tap(String(favourite.id));
      await session.clock('data');
      expect(favourites.size).toBe(0);
      await check();
      await session.type('comment-body', 'A draft comment');
      await check();
      expect(net.unanswered).toEqual([]);
    } finally {
      await session?.close();
      await net.close();
    }
  }, 30000);

  test(`feed and article text remain readable in ${scheme} mode`, async () => {
    const net = await startNet();
    let session;
    try {
      session = await open({ host: 'web', app: 'realworld', env: net.env });
      for (const article of [false, true]) {
        if (article) await session.tap('open-how-to-learn-javascript-efficiently');
        await session.clock('data');
        expect((await session.state()).derives.current.name).toBe(article ? 'article' : 'home');
        if (article) await session.tree('article-title');
        const result = await checkOp(session, [scheme]);
        expect(result.check.flatMap(r => r.findings).filter(f => f.kind === 'contrast')).toEqual([]);
      }
      expect(net.unanswered).toEqual([]);
    } finally {
      await session?.close();
      await net.close();
    }
  }, 30000);
}

test('replies from a previous reader leave the next reader alone', async () => {
  const { answer } = await import('./app.ts');
  const kept = new Map([['realworld.jwt', 'alice']]);
  const store = { get: k => kept.get(k), set: (k, v) => kept.set(k, v), forget: k => kept.delete(k) };
  const held = [];
  const realFetch = globalThis.fetch;
  globalThis.fetch = url => new Promise(resolve => held.push({ url: String(url), resolve }));
  const reply = (i, status, body) => held[i].resolve(new Response(JSON.stringify(body), { status }));
  const settle = () => new Promise(r => setTimeout(r, 0));
  try {
    const alice = answer('currentUser', [], store);
    await settle();
    const login = answer('login', ['bob@example.invalid', 'secret'], store);
    await settle();
    reply(1, 200, { user: { username: 'bob', token: 'bob' } });
    expect((await login).ok).toBe(true);
    reply(0, 401, { errors: { token: ['expired'] } });
    await alice;
    expect(kept.get('realworld.jwt')).toBe('bob');

    const favourite = answer('favorite', ['story', true, 0], store);
    await settle();
    await answer('logout', [], store);
    reply(2, 200, { article: { favorited: true, favoritesCount: 1 } });
    await favourite;
    const shown = answer('article', ['story'], store);
    await settle();
    reply(3, 200, { article: { slug: 'story', title: 'Story', favorited: false, favoritesCount: 0, author: {} } });
    expect(await shown).toMatchObject({ favorited: false, favoritesCount: 0 });
  } finally {
    globalThis.fetch = realFetch;
  }
});
