// A package's configuration (LLP 1133.021 §9 item 7): what an interface's `exact.json` declares under
// `config.<target>` and the app gives under `app.json` `packages.<interface>.<target>`, read at bake, checked the same
// way for every target and written into what that target's code reads (`config` on the web, `context.config` in
// Kotlin and Swift). A value is never printed: a key typed `env` holds a vendor key, and a wrong-typed value may be
// one pasted in by mistake.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { moduleInterfaces } from './typescript.mjs';

const TYPES = new Set(['string', 'number', 'boolean', 'env']);
const FIELDS = new Set(['type', 'required', 'missing', 'metadata']);

const fits = (type, value) => type === 'env' ? value !== null && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).length === 1 && typeof value.env === 'string' && value.env !== ''
  : type === 'number' ? typeof value === 'number' && Number.isFinite(value) : typeof value === type;
const form = (type) => type === 'env' ? '{"env": "<VARIABLE>"}' : `a ${type}`;

/** The name among `names` that differs from `word` only in case, else the nearest within two edits, for a
 * did-you-mean. */
function near(word, names) {
  const distance = (a, b) => {
    let row = [...Array(b.length + 1).keys()];
    for (let i = 1; i <= a.length; i++) {
      const next = [i];
      for (let j = 1; j <= b.length; j++) next[j] = Math.min(row[j] + 1, next[j - 1] + 1, row[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
      row = next;
    }
    return row[b.length];
  };
  const nearest = names.map((n) => [distance(word, n), n]).sort((a, b) => a[0] - b[0])[0];
  return names.find((n) => n.toLowerCase() === word.toLowerCase()) ?? (nearest?.[0] <= 2 ? nearest[1] : null);
}

/** An interface's declaration of key `key` for `target`, normalized: `"tiles": "string"` is `{"type": "string"}`. */
function declaration(name, target, key, d) {
  const at = `${name}'s exact.json "config": {"${target}": {"${key}": …}}`;
  const decl = typeof d === 'string' ? { type: d } : d;
  if (!decl || typeof decl !== 'object' || Array.isArray(decl)) throw new Error(`bake-config-declaration: ${at} is not a type or an object`);
  const extra = Object.keys(decl).find((f) => !FIELDS.has(f));
  if (extra) throw new Error(`bake-config-declaration: ${at} has "${extra}", which is not one of ${[...FIELDS].join(', ')}`);
  if (!TYPES.has(decl.type)) throw new Error(`bake-config-declaration: ${at} has type ${JSON.stringify(decl.type)}, not one of ${[...TYPES].join(', ')}`);
  if (![undefined, true, false, 'release'].includes(decl.required)) throw new Error(`bake-config-declaration: ${at} has "required" ${JSON.stringify(decl.required)}, not true or "release"`);
  if (decl.metadata !== undefined && (target !== 'android' || typeof decl.metadata !== 'string')) throw new Error(`bake-config-declaration: ${at} has "metadata", which names an Android <meta-data> and is only an android key's`);
  return decl;
}

/** Each used interface's configuration for `target`: `{name, extension, declared, values}`, `declared` its keys'
 * normalized declarations and `values` what `app.json` gives them, with each `env` key's variable read (an unset or
 * empty one is absent). Throws, naming the key and never its value: a value whose type is not its declaration's
 * (`bake-config-type`), a key the interface does not declare for `target` (`bake-config-unknown`), and a required key
 * that is absent (its declared `missing` code, else `bake-config-missing`; `"required": "release"` only when `release`).
 * `warn` hears a release-required key absent from a development build, and a `packages` entry naming no installed
 * package. `env` is the build's environment. */
export function packageConfigs(app, target, { extensions, release = false, warn = () => {}, env = process.env }) {
  let manifest = {};
  try { manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8')); } catch { /* a plain Contract directory */ }
  const installed = moduleInterfaces(app), out = [];
  for (const name of Object.keys(manifest.packages ?? {})) {
    if (manifest.packages[name]?.[target] && !installed.some((i) => i.name === name)) warn(`app.json "packages" names ${name}, which is not installed`);
  }
  for (const i of installed.filter((i) => extensions[i.name.split('/').at(-1)] === i.name)) {
    const declared = Object.fromEntries(Object.entries(i.declaration?.config?.[target] ?? {}).map(([key, d]) => [key, declaration(i.name, target, key, d)]));
    const given = manifest.packages?.[i.name]?.[target] ?? {}, values = Object.create(null), at = (key) => `app.json "packages": {"${i.name}": {"${target}": {"${key}": …}}}`;
    for (const key of Object.keys(given)) {
      if (Object.hasOwn(declared, key)) continue;
      const names = Object.keys(declared), guess = near(key, names);
      throw new Error(`bake-config-unknown: ${at(key)}: ${i.name} declares ${names.length ? `no "${key}" for ${target}${guess ? `; did you mean "${guess}"?` : ` (it declares ${names.map((n) => `"${n}"`).join(', ')})`}` : `no configuration for ${target}`}`);
    }
    for (const [key, d] of Object.entries(declared)) {
      let value = Object.hasOwn(given, key) ? given[key] : undefined;
      if (value !== undefined && !fits(d.type, value)) throw new Error(`bake-config-type: ${at(key)} needs ${form(d.type)}${d.type === 'env' ? ', naming the variable that holds the value' : ''}`);
      const variable = d.type === 'env' ? value?.env : null;
      if (d.type === 'env' && value !== undefined) value = env[variable];
      if (value === undefined || value === '') {
        if (d.required === true || (d.required === 'release' && release)) {
          throw new Error(`${d.missing ?? 'bake-config-missing'}: ${at(key)} needs ${d.type === 'env' ? `{"env": "<VARIABLE>"} naming a variable that is set${variable ? `, and ${variable} is not` : ''}` : form(d.type)}`);
        }
        if (d.required === 'release') warn(`${variable ?? `${i.name}'s ${key}`} is ${variable ? 'unset' : 'not given'}, so ${i.name}'s ${key} is absent from this development build (a release build refuses it)`);
        continue;
      }
      values[key] = value;
    }
    out.push({ name: i.name, extension: i.name.split('/').at(-1), declared, values: { ...values } });
  }
  return out;
}

/** [packageConfigs]' values by extension name. */
export const packageConfig = (app, target, options) => new Map(packageConfigs(app, target, options).map((c) => [c.extension, c.values]));

/** The targets whose native code reads its configuration. */
const NATIVE = new Set(['android', 'ios', 'macos', 'tvos']);
const digest = (text) => createHash('sha256').update(text).digest('hex');

/** What a native binary records of `app.json`'s package configuration for `target` (LLP 1133.017 D13: a bake-time
 * value is a binary input): each package's entry by `packages.<name>`, each value by its SHA-256 and an `env` value
 * also by the variable's name, so a key rotated is a new binary and no value is written to a receipt, even one pasted
 * where a variable belongs. */
export function nativePackageConfig(manifest, target, env = process.env) {
  const out = {};
  if (!NATIVE.has(target)) return out;
  for (const [name, targets] of Object.entries(manifest.packages ?? {})) {
    const given = targets?.[target];
    if (!given || typeof given !== 'object') continue;
    out[`packages.${name}`] = Object.fromEntries(Object.entries(given).map(([key, value]) => [key, typeof value?.env === 'string'
      ? { env: value.env, ...(env[value.env] ? { sha256: digest(env[value.env]) } : {}) } : { sha256: digest(JSON.stringify(value)) }]));
  }
  return out;
}

/** The configuration target of a native build for `platform` and its Rust `target` triple. */
export const configTarget = (platform, target = '') => platform === 'ios' && target.includes('-tvos') ? 'tvos' : platform;

/** `text` as a string literal in generated `language` (`kotlin` or `swift`) source: a backslash, a quote and Kotlin's
 * template `$` escaped, and each control or line-separator character by its code point. */
export const sourceString = (text, language) => `"${[...text].map((c) => {
  const code = c.codePointAt(0);
  if (c === '\\' || c === '"' || (c === '$' && language === 'kotlin')) return `\\${c}`;
  if (code < 0x20 || code === 0x7f || code === 0x2028 || code === 0x2029) return language === 'swift' ? `\\u{${code.toString(16)}}` : `\\u${code.toString(16).padStart(4, '0')}`;
  return c;
}).join('')}"`;
