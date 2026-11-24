// A store reader's resource (LLP 1027 D4), which the emitter builds through `reader`, so a plan without one ships none
// of this. A launch whose store holds a granted secret the bake never saw shows a reader's declared `else`, not its
// build-time answer (the bake's empty store's), until its source answers, as the runner's `secrets_unseen` has it
// (runner/src/runner/settlement.rs). An answer kept or carried replaces the build-time answer as before (resources.js
// `res`), so it stands.
import { booting } from './mount.js';
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
  let r, bake;
  if (declared && opened && initial !== undefined) {
    bake = initial;
    const placeholder = ph;
    initial = undefined;
    ph = () => {
      const v = typeof placeholder === 'function' ? placeholder() : placeholder;
      if (v !== undefined) return v;
      // nothing to show: the build-time answer stands in after all
      r.stand = 'build';
      return bake;
    };
  }
  const out = res(name, source, args, initial, initialArgs, type, ph, ...rest);
  r = out.r;
  // Mark a passed-over build-time answer as the launch's to ask again, so that ask keeps it stale, as `res` marks any
  // other; one the page or a reload carried is `res`'s
  if (bake !== undefined && r.value === undefined) {
    r.baked = true;
    // said as that ask starts in the boot, as the runner says of its boot alone
    r.unseen = () => booting() && say(`${name} shows its placeholder until its source answers: the store holds secrets its build-time answer did not see`);
  }
  // a reader before its first answer: a store write asks it again (rt.js `revise`), as the runner's `store_reader`
  r.store = true;
  return out;
};
