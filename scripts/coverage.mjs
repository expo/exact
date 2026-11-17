// coverage — how a test that cannot run here says so.
//
// A test that cannot run is skipped for one of two reasons, and its name says
// which. `missing`: a prerequisite this run should have had (a browser, a
// toolchain, a built fixture). `n/a`: it does not apply here (another
// platform). With EXACT_REQUIRED=1 (the async lane, which owns the coverage of
// its checks) a missing prerequisite fails the test instead of skipping it, so
// a check cannot pass having tested nothing; `n/a` still skips. Without it, a
// developer's machine skips both, naming the reason.
//
// Discovery (chromium(), hermesBundle(), …) stays a pure answer; the policy is
// applied here, where a test is chosen, and can be given explicitly.
import { test as bunTest } from 'bun:test';

/** Whether a missing prerequisite fails rather than skips (EXACT_REQUIRED=1). */
export const required = (env = process.env) => env.EXACT_REQUIRED === '1';

const stand = (fn, test) => Object.assign(fn, { serial: fn, skip: test.skip, todo: test.todo });

/**
 * `test` when nothing is missing; otherwise a stand-in that names
 * `missing <kind>: <reason>` and skips, or under EXACT_REQUIRED fails.
 * `kind` names the prerequisite (chrome, firefox, webkit, hermes, fixture, …).
 * Each skip is a `SKIP:` line on stderr naming the test, since bun prints
 * only a count of skipped tests.
 */
export function need(kind, missing, { test = bunTest, env = process.env } = {}) {
  if (!missing) return test;
  const why = `missing ${kind}: ${missing}`;
  if (required(env)) return stand((name) => test(`${name} — ${why}`, () => { throw new Error(`${why} (EXACT_REQUIRED=1)`); }), test);
  return stand((name, ...args) => { console.warn(`SKIP: ${name} — ${why}`); test.skip(`${name} — ${why}`, ...args); }, test);
}

const ENGINES = ['chromium', 'firefox', 'webkit'];
const kindOf = engine => engine === 'chromium' || engine === 'chrome' ? 'chrome' : engine;
// Imported when asked: the web carrier is heavy, and most tests that use this module drive no browser.
const browserMissing = async engine => (await import('./agent-playwright.mjs')).browserMissing(engine === 'chrome' ? 'chromium' : engine);

/**
 * `need` for each of `engines`, by its Playwright name or the agent's `chrome`:
 * `{ chromium, chrome, firefox, webkit }`. Firefox and WebKit are launched to
 * find out, so name only the engines the file drives.
 */
export async function engineTests(engines = ENGINES, { missing = browserMissing, ...options } = {}) {
  const tests = Object.fromEntries(await Promise.all(engines.map(async engine => [engine, need(kindOf(engine), await missing(engine), options)])));
  return { ...tests, chrome: tests.chromium };
}

/**
 * For one test that drives several engines (`chromium` or the agent's
 * `chrome`, `firefox`, `webkit`): those this machine has. Each missing one is
 * a `SKIP:` line naming the caller's file and line, and the test goes on
 * without it; under EXACT_REQUIRED the test fails instead.
 */
export async function drivable(engines, { env = process.env, missing = browserMissing } = {}) {
  // The caller's frame, read before the first await ends this stack.
  const at = new Error().stack.split('\n')[2]?.match(/([^\s(]+:\d+):\d+\)?$/)?.[1];
  const reasons = await Promise.all(engines.map(missing));
  const why = engines.map((engine, i) => reasons[i] && `missing ${kindOf(engine)}: ${reasons[i]}`);
  if (required(env) && why.some(Boolean)) throw new Error(`${why.filter(Boolean).join('; ')} (EXACT_REQUIRED=1)`);
  engines.forEach((engine, i) => why[i] && console.warn(`SKIP: ${engine} in the test${at ? ` at ${at}` : ''}, whose other parts run — ${why[i]}`));
  return engines.filter((_, i) => !why[i]);
}

/** `test`, or a skip naming why it does not apply here (never a failure). */
export function notApplicable(reason, { test = bunTest } = {}) {
  if (!reason) return test;
  return stand((name, ...args) => test.skip(`${name} — n/a: ${reason}`, ...args), test);
}
