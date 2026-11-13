import { expect, test } from 'bun:test';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';

test('routes restore every screen from its URL', async () => {
  for (const [url, screen] of [
    ['/', 'lock-screen'], ['/home', 'home-screen'], ['/settings', 'settings-screen'],
    ['/settings/display', 'display-screen'], ['/messages', 'messages-list'],
    ['/messages/nico', 'thread-screen'],
  ]) {
    const session = await open({ host: 'web', app: 'typetour', url });
    try {
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(url);
      expect((await session.find(screen)).inactive).not.toBe(true);
      await session.carrier.reset({ keep: true });
      await session.clock('settle');
      expect((await session.state()).navigation.url).toBe(url);
      expect((await session.find(screen)).inactive).not.toBe(true);
    } finally {
      await session.close();
    }
  }
}, 30_000);

test('Settings can lock the phone', async () => {
  const session = await open({ host: 'web', app: 'typetour', url: '/settings' });
  try {
    await session.tap('row-app-lock');
    await session.clock('settle');
    expect((await session.state()).navigation.url).toBe('/');
    expect((await session.find('lock-screen')).inactive).not.toBe(true);
    await session.tap('lock-unlock');
    expect((await session.state()).navigation.url).toBe('/home');
  } finally {
    await session.close();
  }
}, 30_000);

test('the Type Tour shortcut keeps Settings beneath Display', async () => {
  const session = await open({ host: 'web', app: 'typetour', url: '/home' });
  try {
    await session.tap('app-typetour');
    await session.tap('back-settings');
    expect((await session.state()).navigation.url).toBe('/settings');
  } finally {
    await session.close();
  }
});

for (const button of ['specimen-later', 'specimen-install']) {
  for (const settingsFirst of [true, false]) {
    test(`${button} dismisses Display ${settingsFirst ? 'from Settings' : 'from Messages'}`, async () => {
      const session = await open({ host: 'web', app: 'typetour', url: '/home' });
      try {
        await session.tap(settingsFirst ? 'dock-settings' : 'dock-messages');
        const settings = settingsFirst ? (await session.find('settings-screen')).id : null;
        if (settingsFirst) await session.tap('row-display');
        else await session.type('typetour', '/settings/display');
        await session.tap(button);
        expect((await session.state()).navigation.url).toBe('/settings');
        if (settingsFirst) expect((await session.find('settings-screen')).id).toBe(settings);
        await session.tap('typetour', { history: -1 });
        expect((await session.state()).navigation.url).toBe(settingsFirst ? '/home' : '/messages');
      } finally {
        await session.close();
      }
    });
  }
}

test('Display dismissed over Messages keeps Messages beneath Settings', async () => {
  const session = await open({ host: 'web', app: 'typetour', url: '/home' });
  try {
    await session.tap('dock-settings');
    await session.tap('row-app-messages');
    await session.type('typetour', '/settings/display');
    await session.tap('specimen-later');
    expect((await session.state()).navigation.url).toBe('/settings');
    await session.tap('typetour', { history: -1 });
    expect((await session.state()).navigation.url).toBe('/messages');
  } finally {
    await session.close();
  }
});

test("Display's Back names the visit it returns to", async () => {
  const session = await open({ host: 'web', app: 'typetour', url: '/messages' });
  const label = () => session.carrier.evaluate(`document.querySelector('[data-testid="display-screen"] [data-testid="back-settings"]').textContent`);
  try {
    await session.type('typetour', '/settings/display');
    expect(await label()).toContain('Messages');
    await session.tap('back-settings');
    expect((await session.state()).navigation.url).toBe('/messages');
    await session.tap('home-bar');
    await session.tap('dock-settings');
    await session.tap('row-display');
    expect(await label()).toContain('Settings');
  } finally {
    await session.close();
  }
});

test('browser Back restores the visit before switching apps', async () => {
  const session = await open({ host: 'web', app: 'typetour', url: '/settings/display' });
  try {
    await session.tap('back-settings');
    const settings = (await session.find('settings-screen')).id;
    await session.tap('row-app-messages');
    await session.tap('typetour', { history: -1 });
    await session.clock('settle');
    expect((await session.state()).navigation.url).toBe('/settings');
    expect((await session.find('settings-screen')).id).toBe(settings);
    await session.tap('typetour', { history: 1 });
    expect((await session.state()).navigation.url).toBe('/messages');
    await session.tap('typetour', { history: -1 });
    expect((await session.find('settings-screen')).id).toBe(settings);
    await session.tap('row-display');
    await session.tap('back-settings');
    expect((await session.state()).navigation.url).toBe('/settings');
  } finally {
    await session.close();
  }
}, 30_000);

test('browser history visits settings and conversations without losing their actions', async () => {
  const session = await open({ host: 'web', app: 'typetour' });
  const at = async (url, screen) => {
    await session.clock('settle');
    expect((await session.state()).navigation.url).toBe(url);
    expect((await session.find(screen)).inactive).not.toBe(true);
  };
  try {
    await session.tap('lock-unlock');
    await session.tap('dock-settings');
    await session.tap('row-display');
    await at('/settings/display', 'display-screen');
    await session.type('text-size', '1.2');
    await session.tap('typetour', { history: -1 });
    await at('/settings', 'settings-screen');
    await session.tap('typetour', { history: 1 });
    await at('/settings/display', 'display-screen');
    expect((await session.state()).slots.scale).toBe(1.2);
    await session.tap('back-settings');
    await at('/settings', 'settings-screen');
    await session.tap('home-bar');
    await session.tap('dock-messages');
    for (const id of ['nico', 'crew', 'mom', 'james', 'builds', 'ortega']) {
      await session.tap(`conversation-${id}`);
      await at(`/messages/${id}`, 'thread-screen');
      await session.type('composer', 'A message');
      await session.tap('send');
      expect((await session.state()).slots.draft).toBe('');
      await session.tap('thread-back');
      await at('/messages', 'messages-list');
      await session.tap('typetour', { history: 1 });
      await at(`/messages/${id}`, 'thread-screen');
      await session.tap('thread-back');
      await at('/messages', 'messages-list');
    }
    await session.tap('home-bar');
    await session.tap('home-lock');
    await at('/', 'lock-screen');
    await session.tap('notification-nico');
    await at('/messages/nico', 'thread-screen');
    await session.tap('thread-back');
    await at('/', 'lock-screen');
  } finally {
    await session.close();
  }
}, 30_000);

// Build first with `bun host/web/build.mjs typetour`.
for (const appearance of ['system', 'light', 'dark']) {
  test(`Type Tour contrast with ${appearance} appearance`, async () => {
    const session = await open({ host: 'web', app: 'typetour', size: [420, 900],
      env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
    try {
      const failures = [];
      const check = async (screen, part = 'top') => {
        await session.clock('settle');
        await session.tree(screen);
        const url = (await session.state()).navigation.url;
        expect(url).toBe({ 'messages-list': '/messages', 'thread-screen': '/messages/nico',
          'display-screen': '/settings/display', 'settings-screen': '/settings',
          'home-screen': '/home', 'lock-screen': '/' }[screen]);
        const result = await checkOp(session, [appearance === 'system' ? 'both' : appearance]);
        expect(result.check.map(report => report.scheme))
          .toEqual(appearance === 'system' ? ['light', 'dark'] : [appearance]);
        for (const report of result.check) {
          const contrast = report.findings.filter(f => f.kind === 'contrast');
          failures.push(...contrast.map(f => ({ screen, part, scheme: report.scheme, ...f })));
        }
      };
      // Check each viewport of the long Settings and specimen pages.
      const scrollPage = async screen => {
        let more = true;
        while (more) {
          const position = await session.carrier.evaluate(`(() => {
            const root = document.querySelector('[data-testid="${screen}"]');
            const scroller = [...root.querySelectorAll('*')].find(el =>
              ['auto', 'scroll'].includes(getComputedStyle(el).overflowY));
            if (!scroller) throw new Error('Missing scroller in ${screen}');
            const before = scroller.scrollTop;
            scroller.scrollTop += scroller.clientHeight * 0.8;
            return { before, after: scroller.scrollTop,
              more: scroller.scrollTop + scroller.clientHeight < scroller.scrollHeight - 1 };
          })()`);
          expect(position.after).toBeGreaterThan(position.before);
          await check(screen, `scroll ${position.after}`);
          more = position.more;
        }
      };
      await session.tap('lock-unlock');
      await session.tap('app-typetour');
      expect((await session.type('appearance', appearance)).error).toBeUndefined();
      expect((await session.state()).slots.appearance).toBe(appearance);
      // Explicit selections must win even when the device asks for the other scheme.
      if (appearance !== 'system') {
        await session.prefer({ 'prefers-color-scheme': appearance === 'light' ? 'dark' : 'light' });
        expect((await session.op({ op: 'check' })).scheme).toBe(appearance);
      }
      await check('display-screen');
      await scrollPage('display-screen');
      await session.tap('back-settings');
      expect((await session.state()).navigation.url).toBe('/settings');
      await check('settings-screen');
      await scrollPage('settings-screen');
      await session.tap('home-bar');
      await check('home-screen');
      await session.tap('home-lock');
      await check('lock-screen');
      await session.tap('lock-unlock');
      await session.tap('dock-messages');
      await check('messages-list');
      await session.tap('conversation-nico');
      await check('thread-screen');
      await session.type('composer', 'The new type looks good.');
      await session.tree('send');
      expect((await session.state()).slots.draft).toBe('The new type looks good.');
      await check('thread-screen', 'send visible');
      await session.tap('send');
      expect((await session.state()).slots.draft).toBe('');
      expect(failures).toEqual([]);
    } finally {
      await session.close();
    }
  }, 30000);
}

// Static search hints must remain readable even if a role exempts them from checkOp.
test('search hints clear 3:1 in Settings and Messages in both schemes', async () => {
  const session = await open({ host: 'web', app: 'typetour', size: [420, 900],
    env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
  try {
    const readings = [];
    await session.tap('lock-unlock');
    for (const [screen, button] of [['settings', 'dock-settings'], ['messages', 'dock-messages']]) {
      await session.tap(button);
      expect((await session.state()).navigation.url).toBe(`/${screen}`);
      for (const scheme of ['light', 'dark']) {
        await session.prefer({ 'prefers-color-scheme': scheme });
        await session.clock('settle');
        const observed = await session.op({ op: 'check' });
        expect(observed.scheme).toBe(scheme);
        const hints = observed.items.filter(item => item.text === 'Search');
        expect(hints).toHaveLength(1);
        const hint = hints[0];
        expect(hint.y).toBeGreaterThanOrEqual(0);
        expect(hint.y + hint.h).toBeLessThanOrEqual(observed.viewport.h);
        expect(hint.bg).toHaveLength(3);
        // The observer's background includes ancestor fills; composite translucent ink onto it.
        const alpha = hint.fg[3] ?? 1;
        const ink = hint.fg.slice(0, 3).map((channel, i) => channel * alpha + hint.bg[i] * (1 - alpha));
        readings.push({ screen, scheme, contrast: ratio(ink, hint.bg) });
      }
      await session.tap('home-bar');
    }
    expect(readings.filter(reading => reading.contrast < 3)).toEqual([]);
  } finally {
    await session.close();
  }
}, 30000);
