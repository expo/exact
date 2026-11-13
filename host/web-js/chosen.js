// Linked only by plans that call `setLocale` (emit.rs). The locale write
// and exactTime refresh belong to the action's commit, so a refusal undoes
// both. A successful commit keeps the choice beside persisted state under
// "#locale", as runner/src/runner/time.rs does. The next launch reads it
// before state initializes; failed writes retry after successful commits.
import { After, Chosen, Kept, R, Resources, W, locale, refused, say, stood } from './rt.js';

const KEY = Kept('state') + '#locale', TAG = /^[A-Za-z0-9-]{1,64}$/;
let slot = null, kept, unsaved = false;
// A render has no store (LLP 1048.000 D9): its page starts at the base table.
if (!globalThis.__exactRender) try { const v = JSON.parse(localStorage.getItem(KEY)); if (typeof v === 'string' && TAG.test(v)) Chosen.tag = v; } catch {}

/** The locale slot, from its making on: an action at mount (autofocus, a replayed tap) may choose. */
export const localeSlot = s => { slot = s; };

function save() {
  if (globalThis.__exactRender) return;
  try { if (kept) localStorage.setItem(KEY, JSON.stringify(kept)); else localStorage.removeItem(KEY); unsaved = false; }
  catch (e) { if (!unsaved) say(`store #locale failed: ${e}`); unsaved = true; }
}
After.push(() => { if (unsaved && !refused()) save(); });

export function setLocale(tag) {
  if (typeof tag !== 'string' || (tag && !TAG.test(tag))) return say(`setLocale(${JSON.stringify(tag) ?? ''}) refused: a BCP 47 tag ("de", "pt-BR"), or "" for the host's locale`);
  const to = locale(tag || null);
  W(slot, to);
  if (to !== slot.n.v) for (const r of Resources) if (r.source === 'exactTime') R(r);
  stood(() => { kept = tag; Chosen.tag = tag || null; save(); });
}
