// Sources declared in Contract (LLP 1162.001) on the JS target: each answered
// before the app's data module (`data.reserved`, as facts.js answers the
// runner's own), with the browser's `fetch`, as the runner answers them on
// native hosts (runner/src/runner/fetches.rs, runner/src/fetch_template.rs;
// read it beside them). Imported by an app's module only for a plan that
// declares one, so an app whose sources are all declared loads no data module.
// Each row: [method, url template, origin, parameter names, headers, body
// ("", "json" or "graphql"), GraphQL document, JSON Pointer, parameter types,
// answer type, its grant], the types as `sourceTypes` writes them and the grant,
// `net.fetch <origin>`, as the compiler normalizes one.
import { data, Origin, Store } from "./rt.js";
import { FetchError, admitsSecret, appGrantSet, createGrantSet, fetchWith } from "./admission.js";
import { Shaped } from "./shape.js";

const text = v => typeof v === "number" ? String(v) : typeof v === "boolean" ? String(v) : typeof v === "string" ? v : null;
// RFC 6570 §3.2.2: every UTF-8 byte but RFC 3986's unreserved characters, as %XX.
const encode = s => Array.from(new TextEncoder().encode(s), b => /[A-Za-z0-9\-._~]/.test(String.fromCharCode(b)) ? String.fromCharCode(b) : "%" + b.toString(16).toUpperCase().padStart(2, "0")).join("");
/** `template` with each `{…}` filled by `fill`, or null when `fill` gives nothing. */
function expand(template, fill) {
  let out = "", rest = template, at;
  while ((at = rest.indexOf("{")) >= 0) {
    const close = rest.indexOf("}", at), inner = rest.slice(at + 1, close), v = fill(inner);
    if (v == null) return null;
    out += rest.slice(0, at) + v; rest = rest.slice(close + 1);
  }
  return out + rest;
}
/** A runtime value (records and lists are arrays, none is null) as JSON, by its type. */
function json(v, t) {
  if (typeof t === "string") return t === "u" ? null : v;
  if (Array.isArray(t)) return t[0] === "?" ? (v == null ? null : json(v, t[1])) : t[0] === "e" ? v : v.map(x => json(x, t[1]));
  return Object.fromEntries(Object.keys(t).map((f, i) => [f, json(v[i], t[f])]));
}
const shaped = message => Object.assign(new Error(message), { [Shaped]: true });
const kind = j => j === null ? "null" : Array.isArray(j) ? "an array" : typeof j === "object" ? "an object" : `a ${typeof j}`;
/** A server's JSON read into type `t` (D2): a record reads its declared fields and ignores the rest, an absent one is
 * `none` for an option; as js/value's `project_json`. */
function project(j, t, at = "") {
  const fail = want => { throw shaped(`${at ? `field \`${at}\`: ` : ""}expected ${want}, got ${kind(j)}`); };
  if (typeof t === "string") {
    if (t === "n") return typeof j === "number" && Number.isFinite(j) ? j : fail("a number");
    if (t === "b") return typeof j === "boolean" ? j : fail("a bool");
    if (t === "s") return typeof j === "string" ? j : fail("a string");
    return j === null ? null : fail("null");
  }
  if (Array.isArray(t) && t[0] === "?") return j === null || j === undefined ? null : project(j, t[1], at);
  if (Array.isArray(t) && t[0] === "e") return typeof j === "string" ? j : fail("a string");
  if (Array.isArray(t)) return Array.isArray(j) ? j.map(x => project(x, t[1], at)) : fail("an array");
  if (j === null || typeof j !== "object" || Array.isArray(j)) fail("an object");
  return Object.keys(t).map(f => {
    const v = Object.hasOwn(j, f) ? j[f] : undefined;
    if (v === undefined && !(Array.isArray(t[f]) && t[f][0] === "?")) throw shaped(`field \`${f}\` is missing`);
    return project(v ?? null, t[f], f);
  });
}
/** RFC 6901: the value at `pointer`, or undefined. */
function pointer(j, p) {
  if (p === "") return j;
  for (const raw of p.slice(1).split("/")) {
    const token = raw.replaceAll("~1", "/").replaceAll("~0", "~");
    if (Array.isArray(j)) { if (!/^(0|[1-9][0-9]*)$/.test(token)) return undefined; j = j[Number(token)]; }
    else if (j !== null && typeof j === "object" && Object.hasOwn(j, token)) j = j[token];
    else return undefined;
    if (j === undefined) return undefined;
  }
  return j;
}

async function answer(name, [method, url, origin, params, headers, body, document, at, types, result, grant], args, forced, call, asker) {
  const arg = p => text(args[params.indexOf(p)]);
  const own = createGrantSet(grant);
  const init = { method, headers: [] };
  for (const [field, value] of headers) {
    let absent = null;
    const filled = expand(value, e => {
      if (!e.startsWith("secret:")) return arg(e);
      const secret = e.slice(7), set = appGrantSet();
      call.seen.read = true;
      const v = set && admitsSecret(set, secret) ? Store.get(secret) : null;
      if (v == null) absent = secret;
      return v;
    });
    if (filled == null) throw new FetchError("Refused", `${name}: the kept secret \`${absent}\` its header \`${field}\` names is absent or not granted, so nothing was sent`);
    if (/[\r\n\0]/.test(filled)) throw new Error(`${name}: the header \`${field}\` would have a line break`);
    init.headers.push([field, filled]);
  }
  const has = h => init.headers.some(([n]) => n.toLowerCase() === h);
  if (!has("accept")) init.headers.push(["Accept", "application/json"]);
  if (body) {
    const variables = Object.fromEntries(params.map((p, i) => [p, json(args[i], types[i])]));
    init.body = JSON.stringify(body === "graphql" ? { query: document, variables } : variables);
    if (!has("content-type")) init.headers.push(["Content-Type", "application/json"]);
  }
  const target = expand(url, e => { const t = arg(e); return t == null ? null : encode(t); });
  const response = await fetchWith(own, target, init, null, undefined, undefined, asker, false, forced);
  if (response.status < 200 || response.status > 299) throw new Error(`${name}: HTTP ${response.status} from ${origin}`);
  let reply;
  try { reply = JSON.parse(await response.text()); }
  catch (e) { throw new Error(`${name}: the reply is not JSON: ${e?.message ?? e}`); }
  if (body === "graphql" && Array.isArray(reply?.errors) && reply.errors.length) {
    const first = reply.errors[0], code = first?.extensions?.code;
    throw Object.assign(new Error(`${name}: ${typeof first?.message === "string" ? first.message : "the GraphQL operation failed"}`), { reason: typeof code === "string" ? code : "" });
  }
  const found = pointer(reply, at);
  if (found === undefined) throw shaped(`${name}: the reply has nothing at \`${at}\``);
  try { return project(found, result); }
  catch (e) { throw shaped(`${name}: ${e.message}`); }
}

export function fetches(rows) {
  const reserved = (data.reserved ??= {});
  for (const [name, row] of Object.entries(rows)) reserved[name] = (_, args, target, forced) => {
    // Its request is out until it settles (`plain`: held.js counts it, and a render waits for it, as for a
    // TypeScript source's fetch), and a header's secret makes its resource a store reader (rt.js `land`).
    const call = { seen: { read: false }, plain: 1 };
    // Who asked it, a tool, a wake or a package's event and not the app, is checked at each hop (LLP 1133.036 D6),
    // as a TypeScript source's fetch is (ts-data.js `call.origin`).
    const p = answer(name, row, args, forced, call, Origin.now()).finally(() => { call.plain = 0; call.recount?.(); });
    p.call = call;
    return p;
  };
}
