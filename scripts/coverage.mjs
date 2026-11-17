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
import { relative } from 'node:path';

/** Whether a missing prerequisite fails rather than skips (EXACT_REQUIRED=1). */
export const required = (env = process.env) => env.EXACT_REQUIRED === '1';

const stand = (fn, test, each) => Object.assign(fn, { serial: fn, skip: test.skip, todo: test.todo, each });
// A test.each name with one row's values in its placeholders, for the SKIP line
const named = (name, row, index) => { const values = Array.isArray(row) ? [...row] : [row]; return name.replace(/%[sdifjop#]/g, m => String(m === '%#' ? index : values.shift())); };

// Bun prints only a count of skipped tests, so each skip is a SKIP: line on stderr. A reason is said whole once a
// process (a serial run's files share this module; --parallel gives each file its own); each skip then names its kind.
const told = new Set();
function skipped(kind, missing, what) {
  if (!told.has(`${kind}\n${missing}`)) {
    told.add(`${kind}\n${missing}`);
    console.warn(`SKIP: missing ${kind}, so the tests that need it are skipped, not failed. This run lacks it; the code under test is not at fault. ${missing}`);
  }
  console.warn(`SKIP: ${what} — missing ${kind}`);
}

/**
 * `test` when nothing is missing; otherwise a stand-in that skips, naming
 * `missing <kind>`, or under EXACT_REQUIRED fails, naming the reason too.
 * `kind` names the prerequisite (chrome, firefox, webkit, hermes, fixture, …).
 */
export function need(kind, missing, { test = bunTest, env = process.env } = {}) {
  if (!missing) return test;
  if (required(env)) {
    const why = `missing ${kind}: ${missing}`, fail = () => { throw new Error(`${why} (EXACT_REQUIRED=1)`); };
    return stand(name => test(`${name} — ${why}`, fail), test, table => name => test.each(table)(`${name} — ${why}`, fail));
  }
  const skip = (name, ...args) => { skipped(kind, missing, name); test.skip(`${name} — missing ${kind}`, ...args); };
  return stand(skip, test, table => (name, ...args) => {
    table.forEach((row, i) => skipped(kind, missing, named(name, row, i)));
    test.skip.each(table)(`${name} — missing ${kind}`, ...args);
  });
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
 * a `SKIP:` line naming the caller's file and line, once, and the test goes on
 * without it; under EXACT_REQUIRED the test fails instead.
 */
const toldParts = new Set();
export async function drivable(engines, { env = process.env, missing = browserMissing } = {}) {
  // The caller's frame, read before the first await ends this stack.
  const at = new Error().stack.split('\n')[2]?.match(/([^\s(]+:\d+):\d+\)?$/)?.[1];
  const reasons = await Promise.all(engines.map(missing));
  if (required(env) && reasons.some(Boolean)) throw new Error(`${engines.map((engine, i) => reasons[i] && `missing ${kindOf(engine)}: ${reasons[i]}`).filter(Boolean).join('; ')} (EXACT_REQUIRED=1)`);
  engines.forEach((engine, i) => {
    // Once a call site: a helper's loop calls it again and again
    const part = `the ${engine} part of the test${at ? ` at ${relative(process.cwd(), at)}` : ''}`;
    if (reasons[i] && !toldParts.has(part)) { toldParts.add(part); skipped(kindOf(engine), reasons[i], part); }
  });
  return engines.filter((_, i) => !reasons[i]);
}

/** `test`, or a skip naming why it does not apply here (never a failure). */
export function notApplicable(reason, { test = bunTest } = {}) {
  if (!reason) return test;
  return stand((name, ...args) => test.skip(`${name} — n/a: ${reason}`, ...args), test);
}
