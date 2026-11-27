// @ref LLP 1123 D17 — what the web's executor writes on an element: the inline declarations it owns there (D9's
// observations), held per element so that a write of the element's whole inline style keeps them. The runtime replaces
// an element's `cssText` in its `style` op and when motion puts an author's style back; each such write goes through
// `carry`, which drops any copy of an owned declaration from the text (a snapshot taken while it was written) and
// appends the current one, in the same write, so no style change event sees the element without it.

/** Each element's owned declarations, by name, and their text once a carry has built it. */
const owned = new WeakMap();
/** Every name the executor has owned on any element: a style text holding one carries a copy from when it was written
 * (a motion snapshot), which no longer stands once it is dropped. */
const ever = new Set();

/** Write `value` as the element's own `name`, held until `drop`. */
export function own(el, name, value) {
  let held = owned.get(el);
  if (!held) owned.set(el, (held = { names: new Map(), text: null }));
  ever.add(name);
  held.names.set(name, value);
  held.text = null;
  el.style.setProperty(name, value);
}

/** Remove the element's `name` and stop holding it. */
export function drop(el, name) {
  const held = owned.get(el);
  if (!held?.names.delete(name)) return;
  if (held.names.size) held.text = null;
  else owned.delete(el);
  el.style.removeProperty(name);
}

/** `text`, an inline style about to replace the element's, without any copy of a declaration the executor owns or owned
 * (the browser's parser takes them out, so a quoted `;` stays its value's) and with the element's owned declarations as
 * they are now. */
export function carry(el, text) {
  let out = text;
  if (out.includes("--exact-") && [...ever].some((name) => out.includes(name))) {
    scratch ??= document.createElement("div").style;
    scratch.cssText = out;
    for (const name of ever) scratch.removeProperty(name);
    out = scratch.cssText;
  }
  const held = owned.get(el);
  if (!held) return out;
  held.text ??= [...held.names].map(([name, value]) => `;${name}:${value}`).join("");
  return out + held.text;
}
let scratch;

const registered = new Set();
/** Register `names` as properties of `syntax` that do not inherit, `initial` unsaid (CSS Properties and Values API), a
 * `<length>` and `0px` unless said, so a box's value stays its own and the browser resolves it. Whether every one is
 * registered: an engine without `CSS.registerProperty` registers none. */
export function register(names, syntax = "<length>", initial = "0px") {
  if (typeof CSS === "undefined" || typeof CSS.registerProperty !== "function") return false;
  for (const name of names) {
    if (registered.has(name)) continue;
    try {
      CSS.registerProperty({ name, syntax, inherits: false, initialValue: initial });
    } catch {
      // registered already in this document, by an earlier copy of this module
    }
    registered.add(name);
  }
  return true;
}
