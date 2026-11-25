import { bindingMemo, catchUp, cssText, effect, exitOf, end, onEnd, owner, pr, scope, viewId, write, writeItem } from './rt.js';
import { rowKey } from './rows.js';

const EXIT = '--exact-exit-animation', LT = '--exact-layout-transition';
const Exits = new Map(), Fixed = new Map(), Bound = new Map();
let Inspection = 0;

function inspect(f) {
  Inspection++;
  return f();
}

// A probe reads as a binding effect does, including the committed clock. Its derives and dependencies end here.
function read(f) {
  let value;
  const own = scope(() => effect(() => { value = f(); }), null);
  end(own);
  return value;
}

// Inspect outer scopes first: nested subjects may read their current row or match value.
// An arm or row can have left the logical tree while its DOM update is still queued.
function present(own) {
  if (!own) return true;
  if (own.$inspection !== Inspection) {
    const value = !own.gone && present(own.up) && (!own.$presence || own.$presence());
    own.$inspection = Inspection;
    own.$present = value;
  }
  return own.$present;
}

export function presenceArm(view, subject, arm, bound) {
  if (!view) return view;
  return (...args) => {
    // The arm owns this binding cache across reruns of its region effect.
    const current = bindingMemo(subject);
    owner().$presence = () => {
      const value = current();
      if ((bound ? value != null : !!value) !== (arm === 0)) return false;
      if (bound && value != null) write(bound.n, value);
      return true;
    };
    return view(...args);
  };
}

export function presenceRows(row, list, key, identify = rowKey) {
  const current = bindingMemo(() => {
    const rows = new Map(), seen = new Map();
    list().forEach((item, index) => rows.set(identify(key(() => item, () => index), rows, seen), [item, index]));
    return rows;
  });
  return (p, item, index, k) => {
    owner().$presence = () => {
      const value = current().get(k);
      if (!value) return false;
      writeItem(item.n, value[0]);
      write(index.n, value[1]);
      return true;
    };
    return row(p, item, index, k);
  };
}

// Inspection publishes queued row and match values before checking the effect: those writes can make it stale.
// Only S owns a last CSS write; motion-held and reference-resolving bindings keep their full probes.
const quiet = (element, property) => element.$probeFx?.[property]?.s === 0 && !element.$css[property];
export function track(element, property, fx) {
  if (property === EXIT || property === LT) (element.$probeFx ??= {})[property] = fx;
}

// The runner ends an arm without updating its bindings (region.rs). Capture before writes, even when
// shared.js defers the DOM update: its first flush can run several commits' coalesced effects.
// A scope already absent from the logical tree keeps the snapshot taken before its removal.
export function captureExits() {
  if (!Exits.size) return;
  catchUp(() => inspect(() => {
    for (const [element, entry] of Exits) {
      try {
        if (!present(entry.own)) continue;
        element.$exit = quiet(element, EXIT) ? '' : read(entry.binding) ?? '';
      }
      catch {} // The tree update owns a subject's or key's trap; a probe must not prevent the next action.
    }
  }));
}

function registerExit(element, binding) {
  const entries = binding ? Exits : Fixed;
  entries.set(element, { own: owner(), binding });
  onEnd(() => entries.delete(element));
  pr(element);
}

export const px = element => registerExit(element, () => element.style.getPropertyValue(EXIT));
export const fixedExit = element => registerExit(element, null);

// A region can disappear after an earlier effect has moved its siblings. Read its outgoing
// boxes before the tree flush. A virtualized row plays its root's exit on the wrapper.
function departures() {
  return catchUp(() => inspect(() => {
    const leaving = new Set();
    for (const entries of [Exits, Fixed]) for (const [element, entry] of entries) {
      if (!element.isConnected || (entry.binding && !exitOf(element))) continue;
      try {
        if (present(entry.own)) continue;
      } catch {} // A trapped probe leaves the tree update to decide whether the box goes.
      leaving.add(element);
      const parent = element.parentElement;
      if (parent?.hasAttribute('data-listitemkey') && parent.firstElementChild === element) leaving.add(parent);
    }
    return leaving;
  }));
}

export function exitBinding(element, property, unit, binding) {
  if (property === EXIT && Exits.has(element)) {
    Exits.get(element).binding = () => cssText(property, unit, binding());
  }
}

export function lt(element, parse, binding) {
  const own = owner();
  Bound.set(element, { own, will: () => {
    try { return read(() => present(own) && parse(binding())?.css != null); } catch { return false; }
  } });
  onEnd(() => Bound.delete(element));
  pr(element);
}

// As wasm's ops: before the tree update a view gaining the row; after, each whose declaration changed
// (a create: not a restyle, as paint.js keeps a ghost parent's isolation).
export function gains() {
  return catchUp(() => inspect(() => {
    const ops = [];
    for (const [element, entry] of Bound) {
      try {
        if (present(entry.own) && quiet(element, LT)) { element.$lt = false; continue; }
      } catch {} // The binding probe below leaves a trap to the tree update.
      element.$lt = !!element.style.getPropertyValue(LT);
      if (!element.$lt && element.isConnected && entry.will()) ops.push({ op: 'style', id: viewId(element), css: LT });
    }
    return ops;
  }));
}

export function restyled() {
  const ops = [];
  for (const element of Bound.keys()) {
    if (!element.$lt && element.$probeFx?.[LT] && !element.$css[LT]) continue;
    const now = !!element.style.getPropertyValue(LT);
    if (element.isConnected && element.$lt !== now) ops.push({ op: 'create', id: viewId(element) });
    element.$lt = now;
  }
  return ops;
}

/** What rt.js calls through `usePresenceBindings`. */
export const presenceUse = () => ({ capture: captureExits, bind: exitBinding, arm: presenceArm, rows: presenceRows, gains, departures, restyled, track });
