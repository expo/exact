// Only a page with external translations links this module.
import { Chosen, strings, locale, commit, stood, say, inflight, journal, resolvedLocale, W, R, Resources } from './rt.js';
import { LocaleSlot } from './strings.js';
import { adoptAll, checkpoint } from './mount.js';
import { fetchTable } from './fetch-table.js';

export function lookupLanguage(rows, tag, fallback = rows[0]) {
  for (;;) {
    const row = rows.find(r => r[0].toLowerCase() === tag.toLowerCase());
    if (row) return row;
    const i = tag.lastIndexOf('-');
    if (i < 0) return fallback;
    tag = tag.slice(0, i);
  }
}
export const validTag = tag => typeof tag === 'string' && /^[A-Za-z0-9-]{1,64}$/.test(tag);
function storedLanguage(key) {
  try {
    const tag = JSON.parse(localStorage.getItem(key));
    if (validTag(tag)) return tag;
  } catch {}
  return null;
}

const WAIT = 5000;

function requestTable(url, startup = false) {
  const controller = new AbortController();
  const abort = () => controller.abort();
  let request = fetchTable(url, controller.signal);
  if (!startup) {
    let timer;
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('language request timed out')), WAIT);
    });
    request = Promise.race([request, timeout]).finally(() => { clearTimeout(timer); abort(); });
  }
  return {url, request, abort};
}

/** The deadline ends the wait to show a document. Its checkpoint can still arrive later. */
function launchLanguage(wanted, reveal = () => {}) {
  const page = globalThis.exact ??= {};
  let expire, lastShown;
  const launch = {
    wanted, reveal,
    deadline: new Promise(resolve => { expire = resolve; }),
    report(reason, shown) {
      const pair = JSON.stringify([launch.wanted, shown]);
      if (lastShown === pair) return;
      lastShown = pair;
      launch.reported = true;
      (page.journal ??= []).push(`language ${launch.wanted} ${reason}; showing ${shown}`);
    },
    finish() { clearTimeout(timer); reveal(); },
  };
  const at = page.languageDeadline ?? performance.now() + WAIT;
  const remaining = at - performance.now();
  // A page already on screen can activate long after launch. Its entry has no time left to wait.
  if (remaining <= 0) expire();
  const timer = setTimeout(() => {
    launch.timedOut = true;
    expire();
    reveal();
    // A ready entry can mount before the next paint. Otherwise the server's page is visible.
    setTimeout(() => {
      if (page.languageLaunch === launch && (launch.adopting || launch.covered) && document.getElementById('exact-root')?.hasChildNodes())
        launch.report('timed out waiting for adoption', document.documentElement.lang);
    }, 0);
  }, Math.max(0, remaining));
  return page.languageLaunch = launch;
}

export function languageHead(rows, key, browserTag, entry) {
  const page = globalThis.exact ??= {};
  page.languageDeadline = performance.now() + WAIT;
  const choice = key && storedLanguage(key);
  const [lang, file] = lookupLanguage(rows, choice || browserTag);
  const html = document.documentElement;
  const hidden = html.dir && html.lang.toLowerCase() !== lang.toLowerCase();
  page.languageKey = key;
  if (!hidden && !file) return;
  const opacity = html.style.opacity, priority = html.style.getPropertyPriority('opacity'), inert = html.inert;
  let covered = hidden;
  const launch = launchLanguage(lang, () => {
    if (!covered) return;
    covered = false;
    html.inert = inert;
    html.style.setProperty('opacity', opacity, priority);
  });
  launch.covered = hidden;
  if (file) {
    launch.asset = requestTable(new URL(file, document.baseURI).href, true);
    launch.asset.request.catch(() => {}); // The entry can start after the fetch settles.
  }
  if (hidden) {
    html.inert = true;
    html.style.setProperty('opacity', '0', 'important');
    const url = new URL(entry, document.baseURI).href;
    document.head.append(Object.assign(document.createElement('link'), {rel: 'modulepreload', href: url}));
    document.addEventListener('DOMContentLoaded', () => {
      const page = globalThis.exact;
      const ready = page.ready ??= import(url);
      Promise.resolve(ready).catch(error => {
        launch.finish();
        launch.report('adoption failed: ' + error, html.lang);
      });
    }, {once: true});
  }
}

/** Keep verified tables in the slots shared by the initializers and later language changes. */
export function prepareLanguages(rows, base) {
  strings(rows);
  const page = globalThis.exact ??= {};
  const launch = page.languageLaunch ?? launchLanguage(locale());
  launch.adopting = true;
  if (page.journal && page.journal !== journal) journal.push(...page.journal);
  page.journal = journal;
  const pending = new Map(), table = name => rows.find(r => r[0] === name);
  let requested = 0, applied = 0;
  const load = (row, boot) => {
    const file = row[2];
    if (typeof file !== 'string') return;
    if (!pending.has(file)) {
      const url = new URL(file, base).href;
      const asset = boot?.asset?.url === url ? boot.asset : requestTable(url, !!boot);
      if (asset === boot?.asset) delete boot.asset;
      asset.request = asset.request.then(texts => { row[2] = texts; }).finally(() => {
        if (pending.get(file) === asset) pending.delete(file);
      });
      pending.set(file, asset);
    }
    return pending.get(file).request;
  };
  Chosen.change = (name, tag, apply) => {
    const row = table(name);
    if (typeof row[2] !== 'string') {
      stood(() => { applied = ++requested; adoptAll(); });
      return apply(name);
    }
    stood(() => {
      const serial = ++requested;
      inflight.n++;
      const switchLanguage = async () => {
        let next = row;
        for (;;) {
          await load(next);
          if (serial <= applied) return;
          const current = tag ? next : table(locale(null));
          if (current === next) break;
          next = current;
        }
        commit(() => { apply(next[0]); stood(() => { applied = serial; adoptAll(); }); }, 'setLocale');
      };
      switchLanguage().catch(error => say(`setLocale(${JSON.stringify(tag)}) refused: language ${name} failed: ${error}; showing ${resolvedLocale()}`)).finally(() => inflight.n--);
    });
  };
  // Start before waiting for styles or answers. Keep the result even when the entry attaches late.
  const preference = () => page.languageKey ? storedLanguage(page.languageKey) : Chosen.tag;
  const first = table(locale(preference()));
  let initial = load(first, launch);
  initial?.catch(() => {});
  const timeout = new Error('language request timed out');
  return async mount => {
    for (;;) {
      const choice = preference(), wanted = table(locale(choice));
      let shown = wanted;
      launch.wanted = wanted[0];
      let failure;
      const read = wanted === first && initial ? initial : load(wanted, launch);
      initial = null;
      // The deadline releases the mount, while a verified response can still translate the live page.
      if (read) try {
        await Promise.race([read, launch.deadline]);
        if (typeof wanted[2] === 'string') failure = timeout;
      } catch (error) { failure = error; }
      if (wanted[0] !== locale(preference())) continue;
      if (failure) {
        const rendered = document.querySelector('script[type="application/vnd.exact.checkpoint"]');
        shown = rendered ? lookupLanguage(rows, document.documentElement.lang, null) : rows[0];
        launch.report('failed: ' + failure, rendered ? document.documentElement.lang : rows[0][0]);
        launch.reveal();
        if (!shown || typeof shown[2] === 'string') {
          // The server's text is visible. Adopt it when either usable table arrives.
          const available = [];
          if (failure === timeout) available.push(read);
          if (shown && shown !== wanted) available.push(load(shown, launch));
          if (await retryAdoption(available)) {
            for (const row of [wanted, shown]) {
              const file = row?.[2];
              pending.get(file)?.abort();
              pending.delete(file);
            }
            continue;
          }
          if (typeof wanted[2] !== 'string' || wanted[0] !== locale(preference()) ||
              !shown || typeof shown[2] === 'string') continue;
        }
      }
      // Mount offers autofocus synchronously. Restore interaction in the same task, before that offer.
      const html = document.documentElement, lang = html.lang, dir = html.dir;
      const root = document.getElementById('exact-root');
      const backup = root?.firstElementChild && root.cloneNode(true);
      const translating = backup && lang !== shown[0];
      const before = requested, lastApplied = applied;
      // The initializer and the final language effect must see the same table, even if autofocus changes System.
      Chosen.tag = shown[0];
      launch.finish();
      try {
        mount();
        // A sliced list still has the server's text in its waiting rows.
        if (translating) adoptAll();
      } catch (error) {
        if (backup) {
          root.replaceChildren(...backup.childNodes);
          html.lang = lang;
          html.dir = dir;
        }
        throw error;
      } finally {
        if (applied === lastApplied) Chosen.tag = choice;
      }
      if (launch.timedOut && !launch.reported) launch.report('timed out waiting for adoption', document.documentElement.lang);
      delete page.languageLaunch;
      if (requested === before && (locale() !== wanted[0] || failure === timeout)) {
        const next = table(locale());
        try {
          await (next === wanted ? read : load(next));
          if (requested === before && locale() === next[0]) commit(() => {
            W(LocaleSlot, next[0]);
            for (const r of Resources) if (r.source === 'exactTime') R(r);
            stood(adoptAll);
          }, 'place');
        } catch (error) {
          if (requested === before) {
            launch.wanted = next[0];
            launch.report('failed: ' + error, resolvedLocale());
          }
        }
      }
      return;
    }
  };
}

// Retain capture and the original DOM while offline. A reader can retry by activating a control or reconnecting.
function retryAdoption(available = []) {
  return new Promise(resolve => {
    const controller = new AbortController();
    const finish = retry => { controller.abort(); resolve(retry); };
    const retry = () => finish(true);
    for (const event of ['online', 'click', 'keydown', 'input'])
      window.addEventListener(event, retry, {capture: true, signal: controller.signal});
    if (available.length) Promise.any(available).then(() => finish(false), () => {});
  });
}

export async function renderedAnswers(named) {
  const page = globalThis.exact, launch = page.languageLaunch;
  let read = page.answers;
  for (;;) {
    try {
      if (named.dataset.src) {
        const text = await (read ?? fetch(named.dataset.src, {cache: 'no-store'}).then(r => r.ok ? r.text() : null));
        if (!text) throw new Error('checkpoint unavailable');
        named.textContent = text;
      }
      checkpoint();
      return;
    } catch (error) {
      launch.report('checkpoint read failed: ' + error, document.documentElement.lang);
      launch.reveal();
      await retryAdoption();
      read = null;
    }
  }
}
