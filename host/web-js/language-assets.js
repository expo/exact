// Only a page with external translations links this module.
import { Chosen, strings, locale, commit, stood, say, inflight, journal, resolvedLocale, W, R, Resources } from './rt.js';
import { LocaleSlot } from './strings.js';
import { adoptAll, checkpoint } from './mount.js';
import { fetchTable } from './fetch-table.js';

export function lookupLanguage(rows, tag) {
  for (;;) {
    const row = rows.find(r => r[0].toLowerCase() === tag.toLowerCase());
    if (row) return row;
    const i = tag.lastIndexOf('-');
    if (i < 0) return rows[0];
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

function requestTable(url, deadline) {
  const controller = new AbortController();
  let timer;
  const timeout = deadline ?? new Promise(resolve => { timer = setTimeout(resolve, WAIT); });
  const request = Promise.race([fetchTable(url, controller.signal), timeout.then(() => {
    throw new Error('language request timed out');
  })]).finally(() => { clearTimeout(timer); controller.abort(); });
  return {url, request};
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
  const timer = setTimeout(() => {
    launch.timedOut = true;
    expire();
    reveal();
    // A ready entry can mount before the next paint. Otherwise the server's page is visible.
    setTimeout(() => {
      if (page.languageLaunch === launch && (launch.adopting || launch.covered) && document.getElementById('exact-root')?.hasChildNodes())
        launch.report('timed out waiting for adoption', document.documentElement.lang);
    }, 0);
  }, Math.max(0, WAIT - (performance.now() - (page.languageStarted ?? performance.now()))));
  return page.languageLaunch = launch;
}

export function languageHead(rows, key, browserTag, entry) {
  const choice = key && storedLanguage(key);
  const [lang, file] = lookupLanguage(rows, choice || browserTag);
  const html = document.documentElement;
  const hidden = html.dir && html.lang.toLowerCase() !== lang.toLowerCase();
  const page = globalThis.exact ??= {};
  page.languageStarted = performance.now();
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
    launch.asset = requestTable(new URL(file, document.baseURI).href, launch.deadline);
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
      const asset = boot?.asset?.url === url ? boot.asset : requestTable(url, boot?.deadline);
      pending.set(file, asset.request.then(texts => { row[2] = texts; }).finally(() => pending.delete(file)));
    }
    return pending.get(file);
  };
  Chosen.change = (name, tag, apply) => {
    const row = table(name);
    if (typeof row[2] !== 'string') {
      stood(() => { applied = ++requested; adoptAll(); });
      return apply();
    }
    stood(() => {
      const serial = ++requested;
      inflight.n++;
      load(row).then(() => {
        if (serial > applied) commit(() => { apply(); stood(() => { applied = serial; adoptAll(); }); }, 'setLocale');
      }, error => say(`setLocale(${JSON.stringify(tag)}) refused: language ${name} failed: ${error}; showing ${resolvedLocale()}`)).finally(() => inflight.n--);
    });
  };
  // Start before waiting for styles or answers. Keep the result even when the entry attaches late.
  const preference = () => page.languageKey ? storedLanguage(page.languageKey) : Chosen.tag;
  const first = table(locale(preference()));
  let initial = load(first, launch)?.then(() => null, error => error);
  return async mount => {
    for (;;) {
      const choice = preference(), wanted = table(locale(choice));
      let shown = wanted;
      launch.wanted = wanted[0];
      let failure;
      if (wanted === first && initial) failure = await initial;
      else try { await load(wanted); } catch (error) { failure = error; }
      initial = null;
      if (wanted[0] !== locale(preference())) continue;
      if (failure) {
        const rendered = document.querySelector('script[type="application/vnd.exact.checkpoint"]');
        shown = rendered ? table(document.documentElement.lang) : rows[0];
        launch.report('failed: ' + failure, rendered ? document.documentElement.lang : rows[0][0]);
        launch.finish();
        if (shown && shown !== wanted) {
          try { await load(shown); } catch {}
          if (wanted[0] !== locale(preference())) continue;
        }
        if (!shown || typeof shown[2] === 'string') {
          // Retain the server's text until the client can mount in its language or the reader's choice.
          await retryAdoption();
          continue;
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
      if (requested === before && locale() !== wanted[0]) {
        const next = table(locale());
        try {
          await load(next);
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
      if (launch.timedOut && !launch.reported) launch.report('timed out waiting for adoption', document.documentElement.lang);
      delete page.languageLaunch;
      return;
    }
  };
}

// Retain capture and the original DOM while offline. A reader can retry by activating a control or reconnecting.
function retryAdoption() {
  return new Promise(resolve => {
    const controller = new AbortController();
    const retry = () => { controller.abort(); resolve(); };
    for (const event of ['online', 'click', 'keydown', 'input'])
      window.addEventListener(event, retry, {capture: true, signal: controller.signal});
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
