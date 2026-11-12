// LLP 1133.015 §10's acceptance drives for Weatherlight+, under the agent: the implementation is its own substitute,
// so no store is reached and each purchase is a `purchase` ticket the drive taps. Drives 1 to 9 and 11 run on macOS
// and the iOS simulator (`ios`, the simulator EXACT_SIM names), drive 12 on the web; drive 10 is the runner's
// (`runner/src/runner/modules/purchases_tests.rs`). An Open-Meteo response fixture keeps the forecast independent
// of service availability and includes unavailable values beyond the hourly and daily fields the UI needs.
//
// usage: bun packages/purchases/test/drives.mjs <macos|ios|web> [--only 1,2,...]
//   Build first: `bun host/apple/build.mjs weatherlight-apple` (macOS), `--ios --sim <name>` (iOS), or
//   `bun host/web/build.mjs weatherlight` (web).
import { resolve } from 'node:path';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { answer as weatherAnswer } from '../../../host/web-js/conformance/net.mjs';
import { bakeTool } from '../../../scripts/linked-plan.mjs';
import { checker } from '../../camera/test/fixture.mjs';
import { texts } from '../../photos/test/fixture.mjs';

const root = resolve(import.meta.dir, '../../..');
const argv = process.argv.slice(2);
const host = argv.find((a) => ['macos', 'ios', 'web'].includes(a)) ?? 'macos';
const only = argv.includes('--only') ? new Set(argv[argv.indexOf('--only') + 1].split(',').map(Number)) : null;
const wanted = (n) => !only || only.has(n);
const { check, failed } = checker();
const { open } = await import(resolve(root, 'scripts/agent.mjs'));
const EPOCH = 1_767_225_600_000, DAY = 86_400_000;
let stores = 0;

const fixture = host === 'web' ? null : await forecastFixture();
process.on('exit', () => fixture?.close());
const launch = (storage, epoch = EPOCH) => open({ host, app: 'weatherlight', storage, epoch: String(epoch),
  ...(fixture ? { plan: fixture.plan, failFetch: 'https://' } : {}) });
const fresh = () => `purchases-${Date.now().toString(36)}-${stores++}`;

async function forecastFixture() {
  const common = 'temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,weather_code,cloud_cover,wind_speed_10m';
  const query = `latitude=37.7749&longitude=-122.4194&timezone=auto&forecast_days=16&current=${common}&hourly=${common},precipitation_probability&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max,precipitation_probability_max`;
  const payload = weatherAnswer('api.open-meteo.com', 'GET', `/v1/forecast?${query}`).body;
  // Only the HTTP response is substituted. The app's parser, resources,
  // purchase delivery, file storage and review tasks are the shipped code.
  const fetch = `async function fetch(url: string) {
    const params = new URL(url).searchParams;
    // Drive 11 loses the forecast in London, then recovers in Tokyo.
    if (params.get('longitude') === '-0.1278') return { ok: false, status: 503, json: async () => ({}) };
    const span = Number(params.get('forecast_days'));
    if (span !== 7 && span !== 16) throw new Error('Unexpected forecast span');
    const data = JSON.parse(${JSON.stringify(payload)});
    for (const key of Object.keys(data.daily)) data.daily[key] = data.daily[key].slice(0, span);
    for (const key of ['precipitation_probability_max', 'uv_index_max']) data.daily[key].fill(null, 5);
    for (const key of Object.keys(data.hourly)) {
      data.hourly[key] = data.hourly[key].slice(0, span * 24);
      if (key !== 'time') data.hourly[key].fill(null, 48);
    }
    return { ok: true, status: 200, json: async () => data };
  }`;
  const target = resolve(root, 'target');
  mkdirSync(target, { recursive: true });
  const scratch = mkdtempSync(resolve(target, 'purchases-drive-')), source = resolve(scratch, 'source');
  const close = () => rmSync(scratch, { recursive: true, force: true });
  try {
    mkdirSync(source);
    for (const file of ['app.contract', 'app.json', 'strings', 'store']) {
      cpSync(resolve(root, 'apps/weatherlight', file), resolve(source, file), { recursive: true });
    }
    writeFileSync(resolve(source, 'app.ts'), readFileSync(resolve(root, 'apps/weatherlight/app.ts'), 'utf8') + '\n' + fetch);
    const baked = resolve(scratch, 'baked');
    const child = Bun.spawn([bakeTool.built, source, '--out', baked],
      { cwd: root, stdout: 'ignore', stderr: 'pipe' });
    const error = await new Response(child.stderr).text();
    if (await child.exited !== 0) throw new Error(error);
    return { plan: resolve(baked, 'app.plan'), close };
  } catch (error) { close(); throw error; }
}
const lines = async (s) => (await s.logs({ all: true })).lines.join('\n');
const ticket = async (s) => {
  for (let i = 0; i < 40; i++) {
    const t = ((await s.state()).pending ?? []).find((p) => p.device?.capability === 'purchase' || p.device?.capability === 'restore');
    if (t) return t;
    await s.clock('+250 real');
  }
  return null;
};
/** Taps `testId`, then answers the purchase ticket with `choice`; the ticket, or null if none came. */
async function buy(s, testId, choice = 'buy') {
  await s.tap(testId);
  const t = await ticket(s);
  if (t) await s.tap(`@${t.ticket}`, { choice });
  for (let i = 0; i < 4; i++) { await s.clock('+50'); await s.clock('data'); }
  return t;
}
/** The forecast's span once the forecast has arrived: `16-DAY FORECAST` or `7-DAY FORECAST`. */
async function span(s, want, ms = 20000) {
  if (fixture) {
    await s.clock('+1100');
    await s.clock('data');
    const t = await texts(s);
    return t.temperature === '—°' ? `Unavailable: ${t['forecast-days']} ${t.temperature}` : t['forecast-days'];
  }
  const end = Date.now() + ms;
  let got;
  while (Date.now() < end) {
    await s.clock('+1100');
    const t = await texts(s);
    got = `${t['forecast-days']} ${t.temperature}`;
    if (t.temperature !== '—°' && (!want || t['forecast-days']?.startsWith(want))) return t['forecast-days'];
    await s.clock('+300 real');
  }
  return got;
}
const support = async (s) => {
  await s.clock('data');
  if (!(await texts(s))['support-panel']) await s.tap('support');
  await s.clock('data');
};
const token = (log, item) => [...log.matchAll(new RegExp(`transaction (\\d+) for ${item.replace('.', '\\.')}`, 'g'))].at(-1)?.[1];

if (host === 'web') {
  const s = await launch(fresh());
  try {
    const shown = await span(s);
    const t = await texts(s);
    const ext = (await s.state()).extensions ?? {};
    check(host, 'drive 12: no Support row, and the seven-day forecast', t.support === undefined && shown?.startsWith('7-DAY'), `${shown} ${t.support}`);
    check(host, 'drive 12: both modules are listed and bound to no implementation', ext.purchases && !ext.purchases.bound && ext['store-review'] && !ext['store-review'].bound,
      JSON.stringify({ purchases: ext.purchases, review: ext['store-review'] }));
  } finally { await s.close(); }
} else {
  if (wanted(1) || wanted(2)) {
    const storage = fresh();
    let s = await launch(storage);
    try {
      check(host, 'drive 1: the free app shows seven days', (await span(s))?.startsWith('7-DAY'), await span(s));
      await support(s);
      const t = await buy(s, 'plan-yearly');
      check(host, 'drive 1: the yearly plan is a purchase ticket naming its item', t?.args?.options?.itemId === 'plus.yearly' || JSON.stringify(t ?? {}).includes('plus.yearly'), JSON.stringify(t));
      const sixteen = await span(s, '16-DAY');
      const log = await lines(s), tok = token(log, 'plus.yearly');
      check(host, 'drive 1: bought, the sixteen-day forecast', sixteen?.startsWith('16-DAY'), sixteen);
      check(host, 'drive 1: the loop acknowledged it', !!tok && log.includes(`purchases: finished ${tok}`), log.split('\n').filter((l) => /purchases/.test(l)).slice(-6).join('\n'));
      check(host, 'drive 1: the screen says welcome', (await texts(s)).thanks === 'Welcome to Weatherlight+.', (await texts(s)).thanks);
    } finally { await s.close(); }
    if (wanted(2)) {
      s = await launch(storage);
      try {
        const first = await texts(s);
        check(host, 'drive 2: the first frame already shows Weatherlight+ from the kept answer', first.support === '✦  Weatherlight+'
          && /owned shows its kept answer from the last session/.test(await lines(s)), `${JSON.stringify(first.support)}\n${(await lines(s)).split('\n').filter((l) => /kept/.test(l)).join('\n')}`);
        await s.clock('settle').catch(() => {});
        check(host, 'drive 2: unchanged once the module answers', (await span(s, '16-DAY'))?.startsWith('16-DAY') && (await texts(s)).support === '✦  Weatherlight+', (await texts(s)).support);
      } finally { await s.close(); }
    }
  }
  if (wanted(3)) {
    const storage = fresh();
    let s = await launch(storage);
    try {
      await support(s);
      await buy(s, 'plan-monthly', 'pending');
      check(host, 'drive 3: pending says waiting, and no sixteen days', (await texts(s)).thanks === 'Waiting for approval.' && (await span(s))?.startsWith('7-DAY'), JSON.stringify(await texts(s)));
    } finally { await s.close(); }
    s = await launch(storage);
    try {
      await s.prefer({ extension: 'purchases.storeEvent={"op":"approve","itemId":"plus.monthly"}' });
      const shown = await span(s, '16-DAY');
      const log = await lines(s), tok = token(log, 'plus.monthly');
      check(host, 'drive 3: approved while away, delivered by the loop and acknowledged', shown?.startsWith('16-DAY') && !!tok && log.includes(`finished ${tok}`), `${shown}\n${log.split('\n').filter((l) => /purchases/.test(l)).slice(-6).join('\n')}`);
    } finally { await s.close(); }
  }
  if (wanted(4)) {
    let s = await launch(fresh());
    try {
      await support(s);
      await buy(s, 'plan-monthly');
      check(host, 'drive 4: bought monthly', (await span(s, '16-DAY'))?.startsWith('16-DAY'), await span(s));
      await s.clock(`+${31 * DAY}`);
      check(host, 'drive 4: past its expirationTime, seven days with no relaunch', (await span(s, '7-DAY'))?.startsWith('7-DAY'), await span(s));
    } finally { await s.close(); }
    s = await launch(fresh());
    try {
      await support(s);
      await buy(s, 'plan-monthly');
      await span(s, '16-DAY');
      const tok = token(await lines(s), 'plus.monthly');
      await s.prefer({ extension: `purchases.storeEvent={"op":"renew","purchaseToken":"${tok}"}` });
      await s.clock(`+${31 * DAY}`);
      check(host, 'drive 4: renewed first, no gap past the first period', (await span(s, '16-DAY'))?.startsWith('16-DAY'), await span(s));
    } finally { await s.close(); }
  }
  if (wanted(5)) {
    let s = await launch(fresh());
    try {
      await support(s);
      await buy(s, 'plan-yearly');
      await span(s, '16-DAY');
      const tok = token(await lines(s), 'plus.yearly');
      await s.prefer({ extension: `purchases.storeEvent={"op":"revoke","purchaseToken":"${tok}"}` });
      check(host, 'drive 5: revoked while running, seven days at the next answer', (await span(s, '7-DAY'))?.startsWith('7-DAY'), await span(s));
    } finally { await s.close(); }
    const storage = fresh();
    s = await launch(storage);
    let tok;
    try {
      await s.prefer({ extension: 'purchases.fault="finish"' });
      await support(s);
      await buy(s, 'plan-yearly');
      await s.clock('+100');
      tok = token(await lines(s), 'plus.yearly');
    } finally { await s.close(); }
    s = await launch(storage);
    try {
      await s.prefer({ extension: `purchases.storeEvent={"op":"revoke","purchaseToken":"${tok}"}` });
      const shown = await span(s, '7-DAY'), log = await lines(s);
      check(host, 'drive 5: revoked before the relaunch, finished by the implementation and never delivered',
        shown?.startsWith('7-DAY') && log.includes(`finished revoked transaction ${tok}`) && !log.includes(`purchases: finished ${tok}`), `${shown}\n${log.split('\n').filter((l) => /purchases/.test(l)).slice(-6).join('\n')}`);
    } finally { await s.close(); }
  }
  if (wanted(6) || wanted(7)) {
    const storage = fresh();
    let s = await launch(storage);
    try {
      await support(s);
      await buy(s, 'tip');
      await s.clock('+200');
      const one = (await texts(s))['tip-count'];
      await buy(s, 'tip');
      await s.clock('+200');
      const two = (await texts(s))['tip-count'], log = await lines(s);
      check(host, 'drive 6: one tip counts one, the next two, each consumed', one === 'Tips so far: 1. Thank you.' && two === 'Tips so far: 2. Thank you.'
        && (log.match(/purchases: finished \d+/g) ?? []).length >= 2, `${one} / ${two}`);
      if (wanted(7)) {
        await s.prefer({ extension: 'purchases.fault="finish"' });
        await buy(s, 'tip');
        await s.clock('+200');
        check(host, 'drive 7: credited, then the consume fails at the fault point', (await texts(s))['tip-count'] === 'Tips so far: 3. Thank you.', (await texts(s))['tip-count']);
      }
    } finally { await s.close(); }
    if (wanted(7)) {
      s = await launch(storage);
      try {
        await s.clock('+1100');
        await s.clock('+200');
        await support(s);
        const log = await lines(s), tok = token(log, 'tip.small');
        check(host, 'drive 7: after the relaunch the tip is consumed without a second count',
          (await texts(s))['tip-count'] === 'Tips so far: 3. Thank you.' && /purchases: finished \d+/.test(log), `${(await texts(s))['tip-count']} ${tok}`);
      } finally { await s.close(); }
    }
  }
  if (wanted(8)) {
    const storage = fresh();
    let s = await launch(storage);
    try {
      await s.prefer({ extension: 'purchases.fault="finish"' });
      await support(s);
      await buy(s, 'plan-yearly');
    } finally { await s.close(); }
    for (const round of [1, 2]) {
      s = await launch(storage);
      try {
        if (round === 1) await s.prefer({ extension: 'purchases.fault="finish"' });
        await s.clock('+61000');
        const log = await lines(s);
        check(host, `drive 8: relaunch ${round}, still in unfinishedPurchases and journaled a minute on`, /still unfinished a minute on/.test(log) || round === 2,
          log.split('\n').filter((l) => /purchases/.test(l)).slice(-4).join('\n'));
        if (round === 2) check(host, 'drive 8: acknowledged once the fault point is gone', /purchases: finished \d+/.test(log), log.split('\n').filter((l) => /purchases/.test(l)).slice(-4).join('\n'));
      } finally { await s.close(); }
    }
  }
  if (wanted(9)) {
    const storage = fresh();
    let s = await launch(storage);
    try {
      await s.prefer({ extension: 'purchases.fault="finish"' });
      await support(s);
      await buy(s, 'plan-monthly');
    } finally { await s.close(); }
    s = await launch(storage, EPOCH + 31 * DAY);
    try {
      const shown = await span(s, '7-DAY'), log = await lines(s);
      check(host, 'drive 9: expired while unacknowledged, seven days though the queue lists it, and the loop acknowledges it',
        shown?.startsWith('7-DAY') && /purchases: finished \d+/.test(log), `${shown}\n${log.split('\n').filter((l) => /purchases/.test(l)).slice(-4).join('\n')}`);
    } finally { await s.close(); }
  }
  if (wanted(11)) {
    const storage = fresh();
    let asked = [];
    for (let day = 0; day < 6; day++) {
      const s = await launch(storage, EPOCH + day * DAY);
      try {
        await span(s);
        const state = await s.state();
        check(host, `drive 11: day ${day + 1} has a forecast and the persisted visit count`,
          state.resources.outlook.ready && state.resources.visit.days === day + 1,
          JSON.stringify({ forecast: state.resources.outlook.ready, visit: state.resources.visit }));
        for (let i = 0; i < 3; i++) { await s.clock('+1500'); await s.clock('data'); }
        if (day === 4) {
          for (const city of ['city-london', 'city-tokyo']) {
            await s.tap(city);
            await span(s);
            check(host, `drive 11: ${city} ${city === 'city-london' ? 'loses' : 'restores'} the forecast`,
              (await s.state()).resources.outlook.ready === (city !== 'city-london'));
            await s.clock('+3000');
            await s.clock('data');
          }
          check(host, 'drive 11: changing cities after the request keeps the review marked as asked',
            (await s.state()).resources.visit.ask === false,
            JSON.stringify((await s.state()).resources.visit));
        }
        asked.push((await lines(s)).match(/requestReview: requested \(app\)/g)?.length ?? 0);
      } finally { await s.close(); }
      if (day === 0 || day === 4) {
        const again = await launch(storage, EPOCH + day * DAY);
        try {
          await span(again);
          await again.clock('+3000');
          await again.clock('data');
          check(host, `drive 11: reopening day ${day + 1} neither counts it twice nor asks again`,
            (await again.state()).resources.visit.days === day + 1 && !/requestReview: requested/.test(await lines(again)));
        } finally { await again.close(); }
      }
    }
    check(host, 'drive 11: one request, on the fifth day opened, from the app\'s own origin', asked.join(',') === '0,0,0,0,1,0', asked.join(','));
  }
}
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
