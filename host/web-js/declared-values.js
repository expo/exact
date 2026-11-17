// JSON values against the types an interface declares: an error answer's code, a value checked or completed
// against its type, the agent's preferred value laid over an answer, and the handles a value holds.

/** The code of an error answer, `{"error": "POSITION_UNAVAILABLE"}`, which may say more in a `message`, and each
 * platform's own code beside it (`apple`, `android`; LLP 1133.011 D1), as the runner's `error_code`. */
export function errorCode(json) {
  if (json === null || typeof json !== "object" || Array.isArray(json)) return null;
  const keys = Object.keys(json);
  return typeof json.error === "string" && keys.every(k => ["error", "message", "apple", "android"].includes(k)) ? json.error : null;
}

/** A Contract call's options record (its fields in order) as the object an
 * implementation reads: a `none` left out, so an omitted option stays the platform's (D2). */
export function object(value, type) {
  if (value === null || value === undefined) return undefined;
  if (Array.isArray(type)) return type[0] === "?" ? object(value, type[1]) : type[0] === "[" ? value.map(v => object(v, type[1])) : value;
  if (!type || typeof type !== "object" || !Array.isArray(value)) return value;
  const out = {};
  Object.keys(type).forEach((f, i) => { const v = object(value[i], type[f]); if (v !== undefined) out[f] = v; });
  return out;
}

/** `json` with every option field it leaves out `null`, as data code reads a value of `type`. */
export function complete(json, type) {
  if (errorCode(json) || json === null || typeof json !== "object") return json;
  if (Array.isArray(type)) return type[0] === "[" && Array.isArray(json) ? json.map(x => complete(x, type[1])) : type[0] === "?" ? complete(json, type[1]) : json;
  if (typeof type !== "object") return json;
  const out = { ...json };
  for (const [f, t] of Object.entries(type)) out[f] = out[f] === undefined ? (Array.isArray(t) && t[0] === "?" ? null : out[f]) : complete(out[f], t);
  return out;
}

/** The strings at `path` in `v`: `a.b`, and `a[].b` for each element of the list `a`. */
export function handlesAt(v, path) {
  const [head, ...rest] = path.split("."), each = head.endsWith("[]"), at = v?.[each ? head.slice(0, -2) : head];
  const items = each ? (Array.isArray(at) ? at : []) : [at];
  return items.flatMap(x => rest.length ? handlesAt(x, rest.join(".")) : typeof x === "string" ? [x] : []);
}

/** `over`'s fields laid over `v`'s, record by record. */
export function overlay(v, over) {
  if (v === null || typeof v !== "object" || Array.isArray(v) || over === null || typeof over !== "object" || Array.isArray(over)) return over;
  for (const [k, x] of Object.entries(over)) v[k] = overlay(v[k], x);
  return v;
}

/** `v`, a partial overlay of a value of `type`: each field it writes checked, record by record. */
export function partial(v, type, at) {
  if (v !== null && typeof v === "object" && !Array.isArray(v) && type && typeof type === "object" && !Array.isArray(type)) {
    for (const [k, x] of Object.entries(v)) {
      if (!Object.hasOwn(type, k)) throw new Failed(`${at}: field \`${k}\` is not in the shape`, "shape");
      partial(x, type[k], `${at}.${k}`);
    }
    return;
  }
  if (Array.isArray(type) && type[0] === "?") { if (v != null) partial(v, type[1], at); return; }
  value(v, type, at);
}

export function equalJson(a, b) {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every(k => Object.hasOwn(b, k) && equalJson(a[k], b[k]));
}

/** `v` as the runner's serde_json writes it, its objects' keys sorted; "" for no value. */
export function serdeJson(v) {
  if (Array.isArray(v)) return `[${v.map(serdeJson).join(",")}]`;
  if (v && typeof v === "object") return `{${Object.keys(v).sort().map(k => `${JSON.stringify(k)}:${serdeJson(v[k])}`).join(",")}}`;
  return JSON.stringify(v) ?? "";
}

/** Set `path` (`coords.accuracy`) in `v` to `filler` where it is absent. */
export function fill(v, path, filler) {
  const parts = path.split(".");
  let at = v;
  for (let i = 0; i < parts.length; i++) {
    if (at === null || typeof at !== "object") return;
    if (i === parts.length - 1) { if (at[parts[i]] === undefined) at[parts[i]] = filler; return; }
    at = at[parts[i]] ??= {};
  }
}

/** `json` as the runtime holds a value of `type` (a record is its fields in
 * order): an option field it leaves out is none, as an omitted word is. */
export function value(json, type, at) {
  if (typeof type === "string") {
    const ok = type === "n" ? typeof json === "number" && isFinite(json) : type === "b" ? typeof json === "boolean"
      : type === "s" ? typeof json === "string" : json === null;
    if (!ok) throw new Failed(`${at}: expected ${{ n: "a number", b: "a bool", s: "a string" }[type] ?? "null"}`, "shape");
    return json;
  }
  if (Array.isArray(type)) {
    if (type[0] === "?") return json == null ? null : value(json, type[1], at);
    if (type[0] === "e") {
      if (!type[1].includes(json)) throw new Failed(`${at}: ${JSON.stringify(json)} is not one of ${type[1].join(", ")}`, "shape");
      return json;
    }
    if (!Array.isArray(json)) throw new Failed(`${at}: expected an array`, "shape");
    return json.map((x, i) => value(x, type[1], `${at}[${i}]`));
  }
  if (json === null || typeof json !== "object" || Array.isArray(json)) throw new Failed(`${at}: expected an object`, "shape");
  for (const k of Object.keys(json)) if (!Object.hasOwn(type, k)) throw new Failed(`${at}: field \`${k}\` is not in the shape`, "shape");
  return Object.entries(type).map(([f, t]) => {
    if (json[f] === undefined && Array.isArray(t) && t[0] === "?") return null;
    if (json[f] === undefined) throw new Failed(`${at}: field \`${f}\` is missing`, "shape");
    return value(json[f], t, `${at}.${f}`);
  });
}
