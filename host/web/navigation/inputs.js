// @ref LLP 1123 D17 — what the web's executor writes on an element: the inline declarations it owns there (D9's
// observations), held per element so that a write of the element's whole inline style keeps them. The runtime replaces
// an element's `cssText` in its `style` op and when motion puts an author's style back; each such write goes through
// `carry`, which drops any copy of an owned declaration from the text (a snapshot taken while it was written) and
// appends the current one, in the same write, so no style change event sees the element without it.

/** Each element's owned declarations, by name, and their text once a carry has built it. */
const owned = new WeakMap();

/** Write `value` as the element's own `name`, held until `drop`. */
export function own(el, name, value) {
  let held = owned.get(el);
  if (!held) owned.set(el, (held = { names: new Map(), text: null }));
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

/** `text`, an inline style about to replace the element's, with the element's owned declarations as they are now. */
export function carry(el, text) {
  const held = owned.get(el);
  if (!held) return text;
  let out = text;
  if (out.includes("--exact-")) for (const name of held.names.keys()) if (out.includes(name)) out = out.replace(declaration(name), "");
  held.text ??= [...held.names].map(([name, value]) => `;${name}:${value}`).join("");
  return out + held.text;
}

const patterns = new Map();
/** An owned declaration in a style text: a length or a keyword, which holds no `;` */
function declaration(name) {
  let p = patterns.get(name);
  if (!p) patterns.set(name, (p = new RegExp(`(^|;)\\s*${name}\\s*:[^;]*`, "g")));
  return p;
}

const registered = new Set();
/** Register `names` as `<length>` properties that do not inherit, `0px` unsaid (CSS Properties and Values API), so a
 * box's value stays its own and the browser resolves it to pixels. Whether every one is registered: an engine without
 * `CSS.registerProperty` registers none. */
export function register(names) {
  if (typeof CSS === "undefined" || typeof CSS.registerProperty !== "function") return false;
  for (const name of names) {
    if (registered.has(name)) continue;
    try {
      CSS.registerProperty({ name, syntax: "<length>", inherits: false, initialValue: "0px" });
    } catch {
      // registered already in this document, by an earlier copy of this module
    }
    registered.add(name);
  }
  return true;
}
