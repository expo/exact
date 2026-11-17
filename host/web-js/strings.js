// Localized strings (LLP 1060).
import { reportPlace } from './navigation.js';
import { Refusal } from './shape.js';
import { commit, effect, W, R } from './rt.js';
import { Resources } from './resources.js';
// The plan's tables, base first: [name, rtl, {key: text}]. The locale slot
// holds the table the viewer's launch locale reads before state initializes;
// `t` reads that table, else the base.
let Texts = [];
// The language the app chose with `setLocale` (chosen.js), null while it follows the page's.
export const Chosen = { tag: null };
export function strings(tables) {
  Texts = tables;
}
/** RFC 4647 lookup of `tag`, by default the app's chosen language, else the page's locale (an agent's `?locale`): the
 * longest subtag prefix a table is named for, without case, else the base. A render has no launch locale: the base
 * (LLP 1060 D4). */
export function locale(tag = Chosen.tag) {
  if (tag == null) {
    tag = Texts[0]?.[0] ?? '';
    if (!globalThis.__exactRender)
      try {
        tag = reportPlace().split('\0')[0];
      } catch {}
  }
  for (;;) {
    const t = Texts.find(r => r[0].toLowerCase() === tag.toLowerCase());
    if (t) return t[0];
    const i = tag.lastIndexOf('-');
    if (i < 0) return Texts[0]?.[0] ?? '';
    tag = tag.slice(0, i);
  }
}
const table = name => Texts.find(r => r[0] === name);
/** `t(key, name=value…)`: MF2 simple messages, `{name}` or `{$name}`; an unfilled name keeps its spelling; `\{ \} \\`
 * escape. budget.js's `x_t` bounds it. */
export function text(name, key, pairs, emit) {
  const text = table(name)?.[2][key] ?? Texts[0][2][key];
  if (text == null) throw new Refusal(`t: no text ${key}`);
  const out = [],
    put = emit ?? (piece => out.push(piece)),
    at = n => {
      for (let i = 0; i < pairs.length; i += 2) if (pairs[i] === n) return pairs[i + 1];
    };
  let from = 0;
  for (const m of text.matchAll(/\\([{}\\])|\{\s*\$?([A-Za-z_][\w-]*)\s*\}/g)) {
    put(text.slice(from, m.index));
    put(m[1] ?? at(m[2]) ?? m[0]);
    from = m.index + m[0].length;
  }
  put(text.slice(from));
  return out;
}
/** The resolved table sets the document's `lang` and `dir` (LLP 1060, ruled). */
let LocaleSlot = null;
/** `exactTime.resolvedLocale`: the table the strings read, "" with none (runner/src/runner/time.rs). */
export const resolvedLocale = () => resolvedLanguage()?.[0] ?? '';
/** The resolved table's `[lang, dir]`, which a rendered page carries (render.mjs), or null with no table. */
export const resolvedLanguage = () => {
  const t = LocaleSlot && (table(LocaleSlot()) ?? Texts[0]);
  return t ? [t[0], t[1] ? 'rtl' : 'ltr'] : null;
};
export function language(slot) {
  LocaleSlot = slot;
  // Set the locale slot and refresh exactTime in one commit, as the host's first set_place does
  commit(() => {
    W(slot, locale());
    for (const r of Resources) if (r.source === 'exactTime') R(r);
  }, 'place');
  if (typeof document !== 'object' || !document.documentElement) return;
  effect(() => {
    [document.documentElement.lang, document.documentElement.dir] = resolvedLanguage();
  });
}
