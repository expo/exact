import { bindingMemo, catchUp, cssText, effect, end, onEnd, owner, pr, scope, viewId, write, writeItem } from './rt.js';
import { rowKey } from './rows.js';

const EXIT = '--exact-exit-animation', LT = '--exact-layout-transition';
const Exits = new Map(), Bound = new Map();
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
  return view && ((...args) => {
    owner().$presence = () => {
      const value = subject();
      if ((bound ? value != null : !!value) !== (arm === 0)) return false;
      if (bound && value != null) write(bound.n, value);
      return true;
    };
    return view(...args);
  });
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
    return row(p, item, index);
  };
}

// The runner ends an arm without updating its bindings (region.rs). Capture before writes, even when
// shared.js defers the DOM update: its first flush can run several commits' coalesced effects.
// A scope already absent from the logical tree keeps the snapshot taken before its removal.
export function captureExits() {
  catchUp(() => inspect(() => {
    for (const [element, entry] of Exits) {
      try { read(() => { if (present(entry.own)) element.$exit = entry.binding() ?? ''; }); }
      catch {} // The tree update owns a subject's or key's trap; a probe must not prevent the next action.
    }
  }));
}

export function px(element) {
  Exits.set(element, { own: owner(), binding: () => element.style.getPropertyValue(EXIT) });
  onEnd(() => Exits.delete(element));
  pr(element);
}

export function exitBinding(element, property, unit, binding) {
  if (property === EXIT && Exits.has(element)) {
    Exits.get(element).binding = () => cssText(property, unit, binding());
  }
}

export function lt(element, parse, binding) {
  const own = owner();
  Bound.set(element, () => {
    try { return read(() => present(own) && parse(binding())?.css != null); } catch { return false; }
  });
  onEnd(() => Bound.delete(element));
  pr(element);
}

// As wasm's ops: before the tree update a view gaining the row; after, each whose declaration changed
// (a create: not a restyle, as paint.js keeps a ghost parent's isolation).
export function gains() {
  return catchUp(() => inspect(() => {
    const ops = [];
    for (const [element, will] of Bound) {
      element.$lt = !!element.style.getPropertyValue(LT);
      if (!element.$lt && element.isConnected && will()) ops.push({ op: 'style', id: viewId(element), css: LT });
    }
    return ops;
  }));
}

export function restyled() {
  const ops = [];
  for (const element of Bound.keys()) {
    const now = !!element.style.getPropertyValue(LT);
    if (element.isConnected && element.$lt !== now) ops.push({ op: 'create', id: viewId(element) });
    element.$lt = now;
  }
  return ops;
}

/** What rt.js calls through `usePresenceBindings`. */
export const presenceUse = () => ({ capture: captureExits, bind: exitBinding, arm: presenceArm, rows: presenceRows, gains, restyled });
