// A store reader's resource (LLP 1027 D4), which the emitter builds through `reader`, so a plan without one ships none
// of this. A launch whose store holds a granted secret the bake never saw shows a reader's declared `else`, not its
// build-time answer (the bake's empty store's), until its source answers, as the runner's `secrets_unseen` has it
// (runner/src/runner/settlement.rs). An answer kept or carried replaces the build-time answer as before (resources.js
// `res`), so it stands.
import { Kept, say } from './rt.js';

let opened = false;

/** `names`: the secrets the app grants (build.mjs). Called before the app runs, so it sees the store as the page
 * opened it. */
export function install(names) {
  try {
    opened = names.some(name => localStorage.getItem(Kept('secret') + name) !== null);
  } catch {}
}

/** `res`, or a development page's `devResource`, for a store reader; `declared`: whether it declares an `else`. With
 * no build-time answer, the resource shows its placeholder while its source is not ready (resources.js `hold`); a
 * placeholder with nothing to show leaves the build-time answer. */
export const reader = (res, declared) => (name, source, args, initial, initialArgs, type, ph, ...rest) => {
  if (declared && opened) {
    const bake = initial;
    let said = false;
    const placeholder = ph;
    initial = undefined;
    ph = () => {
      const v = typeof placeholder === 'function' ? placeholder() : placeholder;
      if (v === undefined) return bake;
      if (!said) {
        said = true;
        say(`${name} shows its placeholder until its source answers: the store holds secrets its build-time answer did not see`);
      }
      return v;
    };
  }
  const out = res(name, source, args, initial, initialArgs, type, ph, ...rest);
  // a reader before its first answer: a store write asks it again (rt.js `revise`), as the runner's `store_reader`
  out.r.store = true;
  return out;
};
