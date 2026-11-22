// Persisted state (LLP 1116 D5): `state … persist` kept in the page's
// `localStorage` under "exact.state.<name>". `persist window` uses the
// tab's `sessionStorage` under "exact.state.#window.<name>" (a development build's own
// prefix, rt.js `Kept`, LLP 1116.004 D8), as the runner keeps it
// (runner/src/runner/persist.rs): JSON text, a number as JSON's (non-finite
// ones as "NaN", "Infinity", "-Infinity", -0 as `-0`), an option as `null`
// or its value, a list as an array. Read as the slot is made, before the
// first commit; after every commit that stands the store holds each
// persisted slot's value. Types are the emitter's `type_json`: "n", "s",
// "b", ["?", t], ["[", t].
import { After, Kept as kept, W, recommit, say } from './rt.js';

const LIMIT = 16384, APP = kept('state'), WINDOW = APP + '#window.', Kept = [], Initial = new Map();
const NONFINITE = { NaN: NaN, Infinity: Infinity, '-Infinity': -Infinity };
const bytes = text => new TextEncoder().encode(text).length;
const name = t => typeof t === 'string' ? { n: 'number', s: 'string', b: 'bool' }[t] : `${t[0] === '?' ? 'option' : 'list'}<${name(t[1])}>`;

/** `v`, parsed JSON, as a value of type `t`; `undefined` when it is not one. */
function revive(v, t) {
  if (t === 'n') return typeof v === 'number' ? v : typeof v === 'string' && Object.hasOwn(NONFINITE, v) ? NONFINITE[v] : undefined;
  if (t === 's') return typeof v === 'string' ? v : undefined;
  if (t === 'b') return typeof v === 'boolean' ? v : undefined;
  if (t[0] === '?') return v === null ? null : revive(v, t[1]);
  if (t[0] !== '[' || !Array.isArray(v)) return undefined;
  const out = v.map(x => revive(x, t[1]));
  return out.includes(undefined) ? undefined : out;
}

function encode(v, t) {
  if (t === 'n') return Number.isFinite(v) ? (Object.is(v, -0) ? '-0' : JSON.stringify(v)) : `"${v}"`;
  if (typeof t === 'string') return JSON.stringify(v);
  if (t[0] === '?') return v == null ? 'null' : encode(v, t[1]);
  return `[${v.map(x => encode(x, t[1])).join(',')}]`;
}

/** Persisted slot `slot`'s first value: the stored one where it fits type `t`, else `initial`. */
export function stored(slot, initial, t, windowScoped = false) {
  Initial.set(slot, initial);
  let text = null;
  try { text = windowScoped ? sessionStorage.getItem(WINDOW + slot) : localStorage.getItem(APP + slot); } catch {}
  if (text == null) return initial;
  let v;
  try { if (bytes(text) <= LIMIT) v = revive(JSON.parse(text), t); } catch {}
  if (v !== undefined) return v;
  say(`persisted state ${slot}: the stored value does not fit ${name(t)}: the initial value stands`);
  return initial;
}

/** Keep signal `s` (persisted slot `slot` of type `t`) in the store after each commit. */
export function keep(s, slot, t, windowScoped = false) {
  const area = windowScoped ? 'sessionStorage' : 'localStorage', key = (windowScoped ? WINDOW : APP) + slot;
  let last = null;
  try { last = globalThis[area].getItem(key); } catch {}
  Kept.push({ s, slot, t, area, key, last, failed: null });
  if (Kept.length === 1) {
    After.push(flush);
    globalThis.addEventListener?.('storage', peer);
  }
}

/** Another tab of the app wrote a persisted slot, or removed it (LLP 1030.004 D2): the value is app-wide, as a Mac app's
 * `UserDefaults` are, so this tab takes it, or the initial value, in a commit of its own. */
function peer(e) {
  for (const k of Kept) {
    if (e.key != null && e.key !== k.key) continue;
    try { if (e.storageArea !== globalThis[k.area]) continue; } catch { continue; }
    const text = e.key == null ? null : e.newValue;
    if (text === k.last) continue;
    let v;
    try { v = text == null ? Initial.get(k.slot) : bytes(text) <= LIMIT ? revive(JSON.parse(text), k.t) : undefined; } catch {}
    if (v === undefined) continue;
    k.last = text;
    recommit(() => W(k.s, v), "another tab's persisted state");
  }
}

function flush() {
  for (const k of Kept) {
    const text = encode(k.s.n.v, k.t);
    if (text !== k.failed) k.failed = null;
    if (text === k.last) continue;
    if (bytes(text) > LIMIT) { say(`persisted state ${k.slot}: ${bytes(text)} bytes is over the ${LIMIT} a persisted value may hold: not kept (the store keeps its last value)`); k.last = text; continue; }
    try { globalThis[k.area].setItem(k.key, text); k.last = text; k.failed = null; say(`store ${k.key}`); }
    catch (e) { if (k.failed !== text) { k.failed = text; say(`persisted state ${k.slot}: write failed: ${e}`); } }
  }
}
