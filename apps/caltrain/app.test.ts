import { expect, test } from 'bun:test';
import { open } from '../../scripts/agent.mjs';
import { checkOp } from '../../scripts/agent-check.mjs';

test('routes restore the station picker and deck from their URLs', async () => {
  for (const [url, screen] of [['/stations', 'stations-screen'], ['/web-deck', 'deck-screen']]) {
    const app = await open({ host: 'web', app: 'caltrain', url });
    try {
      await app.clock('settle');
      expect((await app.state()).navigation.url).toBe(url);
      expect((await app.find(screen)).inactive).not.toBe(true);
      if (url === '/web-deck') {
        await app.clock('until state deckMessage == "deck-ready" within 5000');
        expect((await app.state()).slots.deckLoaded).toBe(true);
        await app.tap('deck-frame', { selector: '#deck-title' });
        await app.clock('until state deckMessage == "deck-tapped" within 5000');
      }
      await app.carrier.reset({ keep: true });
      await app.clock('settle');
      expect((await app.state()).navigation.url).toBe(url);
      expect((await app.find(screen)).inactive).not.toBe(true);
    } finally {
      await app.close();
    }
  }
}, 30_000);

test('browser Back and Forward preserve station selection and the deck', async () => {
  const app = await open({ host: 'web', app: 'caltrain' });
  const at = async (url: string, screen: string) => {
    await app.clock('settle');
    expect((await app.state()).navigation.url).toBe(url);
    expect((await app.find(screen)).inactive).not.toBe(true);
  };
  try {
    await app.tap('change-station');
    await at('/stations', 'stations-screen');
    await app.tap('caltrain-main', { history: -1 });
    await at('/', 'home-screen');
    await app.tap('caltrain-main', { history: 1 });
    await at('/stations', 'stations-screen');
    await app.type('station-search', 'Palo');
    await app.tap('station-paloalto');
    await at('/', 'home-screen');
    expect((await app.find('station-name')).props.text).toBe('Palo Alto');
    expect((await app.state()).slots.query).toBe('');
    await app.tap('open-deck');
    await at('/web-deck', 'deck-screen');
    await app.tap('deck-back');
    await at('/', 'home-screen');
    await app.tap('caltrain-main', { history: 1 });
    await at('/web-deck', 'deck-screen');
    await app.tap('deck-back');
    await at('/', 'home-screen');
    expect((await app.find('station-name')).props.text).toBe('Palo Alto');
    await app.tap('change-station');
    await app.tap('open-deck');
    await at('/web-deck', 'deck-screen');
    await app.tap('caltrain-main', { history: -1 });
    await at('/stations', 'stations-screen');
    await app.tap('stations-back');
    await at('/', 'home-screen');
    await app.tap('change-station');
    await app.type('station-search', 'Palo');
    await app.tap('open-deck');
    await app.tap('deck-back');
    await at('/stations', 'stations-screen');
    expect((await app.state()).slots.query).toBe('Palo');
    await app.tap('stations-back');
    await at('/', 'home-screen');
    expect((await app.state()).slots.query).toBe('');
  } finally {
    await app.close();
  }
}, 30_000);

// Build first: bun host/web/build.mjs caltrain.
test('the plain home and both decks stay readable in both schemes', async () => {
  const app = await open({ host: 'web', app: 'caltrain' });
  const findings: string[] = [];
  const check = async (screen: string) => {
    const result = await checkOp(app, ['both']);
    for (const run of result.check) {
      for (const finding of run.findings) findings.push(`${screen} ${run.scheme}: ${finding.line}`);
    }
  };
  try {
    await app.tap('open-deck');
    await check('deck over sky');
    await app.tap('deck-back');
    await app.tap('sky-toggle');
    await check('plain home');
    await app.tap('deck-toggle');
    await check('plain departure deck');
    await app.tap('open-deck');
    await check('plain deck');
    expect(findings).toEqual([]);
  } finally {
    await app.close();
  }
}, 30_000);
