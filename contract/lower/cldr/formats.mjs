// Writes formats.json: each locale's number and date data, as the pinned
// Bun's `Intl` (ICU 78.1, CLDR 48) formats it, for the compiler to bake into a
// plan for the locales an app ships strings tables for (LLP 1054.000.003 D6).
// Every field is read back from `formatToParts` of probe values, so the data
// is what `Intl` prints, not a reading of CLDR's own patterns.
//
//   bun contract/lower/cldr/formats.mjs           formats.json
//   bun contract/lower/cldr/formats.mjs --oracle  the oracle rows of
//                                                 runner/tests/it/format_locales.json
//
// The record's fields, in `record`'s order, are `contract/lower/src/formats.rs`'s `FIELDS`. Dates
// are the Gregorian calendar's (`-u-ca-gregory`) in every locale; digits are
// the locale's default numbering system's. A locale that needs what the
// runner does not do (a pattern field it does not know, a plural form beyond
// compact numbers) fails here by name and is left out, so it formats as its
// nearest listed parent.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = resolve(dirname(fileURLToPath(import.meta.url)), "formats.json");
// The roster's currency codes and their fraction digits (runner/src/format/mod.rs `CURRENCIES`).
const CODES = { USD: 2, EUR: 2, GBP: 2, JPY: 0, CNY: 2, INR: 2, CAD: 2, AUD: 2, NZD: 2, HKD: 2, SGD: 2, CHF: 2, SEK: 2, NOK: 2, DKK: 2, PLN: 2, MXN: 2, BRL: 2, KRW: 0, ZAR: 2, TWD: 2, ILS: 2, PHP: 2, VND: 0 };
const CATEGORY = { zero: "z", one: "o", two: "t", few: "f", many: "m", other: "x" };

if (process.versions.icu !== "78.1") throw new Error(`ICU ${process.versions.icu}: the oracle rows are ICU 78.1's (package.json's Bun)`);

/** ICU's locales as Intl resolves them: a tag is listed when it resolves to itself. */
function locales() {
  const own = (tag) => {
    try {
      // A number format is the cheaper to make; a date format only when it resolves.
      return new Intl.NumberFormat(tag).resolvedOptions().locale === tag
        && new Intl.DateTimeFormat(tag).resolvedOptions().locale === tag;
    } catch { return false; }
  };
  const az = "abcdefghijklmnopqrstuvwxyz", AZ = az.toUpperCase();
  const languages = [];
  for (const a of az) for (const b of az) {
    if (own(a + b)) languages.push(a + b);
    for (const c of az) if (own(a + b + c)) languages.push(a + b + c);
  }
  const regions = ["001", "150", "419"];
  for (const a of AZ) for (const b of AZ) regions.push(a + b);
  const scripts = ["Adlm", "Arab", "Beng", "Cyrl", "Deva", "Dsrt", "Ethi", "Grek", "Guru", "Hans", "Hant", "Hebr", "Jpan", "Kore", "Latn", "Mong", "Mtei", "Nkoo", "Olck", "Rohg", "Shaw", "Tfng", "Vaii", "Yiii"];
  const out = [];
  for (const l of languages) {
    const bases = [l];
    for (const s of scripts) if (own(`${l}-${s}`)) bases.push(`${l}-${s}`);
    for (const b of bases) {
      out.push(b);
      for (const r of regions) if (own(`${b}-${r}`)) out.push(`${b}-${r}`);
    }
  }
  return out;
}

const UTC = (y, mo, d, h = 0, mi = 0) => {
  const t = new Date(0);
  t.setUTCFullYear(y, mo - 1, d);
  t.setUTCHours(h, mi, 0, 0);
  return t;
};
const BODY = new Set(["integer", "group", "decimal", "fraction"]);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function record(l) {
  const fail = (why) => { throw new Error(`${l}: ${why}`); };
  const nf = new Intl.NumberFormat(l);
  const digits = Array.from(new Intl.NumberFormat(l, { useGrouping: false }).format(1234567890).replace(/[^\p{Nd}]/gu, ""));
  const native = [...digits.slice(9), ...digits.slice(0, 9)];
  const ascii = native.join("") === "0123456789";
  const toAscii = (s) => Array.from(s, (c) => { const i = native.indexOf(c); return i < 0 ? c : String(i); }).join("");
  const parts = (f, n) => f.formatToParts(n);
  const literal = (v) => (/[{}]/.test(v) ? fail(`a brace in ${JSON.stringify(v)}`) : v);
  // A number's template: its body (integer, groups, decimal, fraction) is `{#}`.
  const template = (ps) => {
    let out = "", body = 0, last = "";
    for (const p of ps) {
      if (BODY.has(p.type)) {
        if (!BODY.has(last)) { out += "{#}"; body++; }
      } else if (p.type === "currency") out += "{¤}";
      else if (["literal", "minusSign", "percentSign", "compact"].includes(p.type)) out += literal(p.value);
      else fail(`number part ${p.type}`);
      last = p.type;
    }
    return body === 1 ? out : fail(`a number body ${body} times`);
  };
  const sym = (type) => parts(nf, -1234567.5).find((p) => p.type === type)?.value ?? "";
  const pf = new Intl.NumberFormat(l, { style: "percent" });
  // An amount of money has its own symbols and grouping, which CLDR may set
  // apart from a number's (`de-AT`: `€ 1.234,50` beside `1 234,5`) and by
  // currency (`en-150`: `1.234,50 €`, `1,234.50 US$`).
  const grouping = (f, scale = 1) => {
    const ints = parts(f, 1234567890123 / scale).filter((p) => p.type === "integer").map((p) => Array.from(p.value).length);
    if (ints.length === 1) return "331"; // not grouped: the group symbol is ""
    const primary = ints.at(-1), secondary = ints.length > 2 ? ints.at(-2) : primary;
    if (ints.slice(1, -1).some((n) => n !== secondary) || primary > 9 || secondary > 9) fail("irregular grouping");
    return `${primary}${secondary}${parts(f, 1234 / scale).some((p) => p.type === "group") ? 1 : 2}`;
  };
  const currencyTemplates = [], currencies = [];
  for (const [code, fraction] of Object.entries(CODES)) {
    const f = new Intl.NumberFormat(l, { style: "currency", currency: code });
    if (f.resolvedOptions().maximumFractionDigits !== fraction) fail(`${code} has other fraction digits`);
    const big = parts(f, 1234567890123.5);
    const group = big.find((p) => p.type === "group")?.value ?? "";
    const decimal = parts(new Intl.NumberFormat(l, { style: "currency", currency: code, minimumFractionDigits: 2 }), 1.5).find((p) => p.type === "decimal").value;
    const pair = [template(parts(f, 1234.5)), template(parts(f, -1234.5))];
    let i = currencyTemplates.findIndex((t) => t[0] === pair[0] && t[1] === pair[1]);
    if (i < 0) i = currencyTemplates.push(pair) - 1;
    currencies.push([String(i), literal(parts(f, 1).find((p) => p.type === "currency").value), literal(decimal), literal(group), grouping(f)]);
  }
  // Compact: for 10^0 up to the last magnitude with a pattern of its own (a
  // larger number shows more digits in the last one's), none ([]) or the
  // integer digits kept and the template the shown number takes (`other`'s
  // when it can be shown), then each plural category's that differs from it.
  const cf = new Intl.NumberFormat(l, { notation: "compact", roundingMode: "trunc", signDisplay: "negative" });
  const pr = new Intl.PluralRules(l);
  const compact = [];
  let plural = false;
  for (let k = 0; k <= 20; k++) {
    const ps = parts(cf, 10 ** k);
    if (!ps.some((p) => p.type === "compact")) { compact.push([]); continue; }
    const z = ps.filter((p) => p.type === "integer").map((p) => Array.from(p.value).length).reduce((a, b) => a + b, 0);
    const shown = [];
    // Every last two digits: an integer's plural category repeats by them (checked below).
    for (let d = 10 ** (z - 1); d < Math.min(10 ** z, 10 ** (z - 1) + 100); d++) shown.push(d);
    if (z === 1) for (let d = 11; d < 100; d++) if (d % 10) shown.push(d / 10);
    const byCategory = {};
    for (const d of shown) {
      const t = template(parts(cf, Number(`${d}e${k - z + 1}`)));
      const c = CATEGORY[pr.select(d)];
      if (byCategory[c] === undefined) byCategory[c] = t;
      else if (byCategory[c] !== t) fail(`compact 10^${k} ${d}: ${t} against ${byCategory[c]}`);
    }
    const other = byCategory.x ?? Object.values(byCategory)[0];
    const rest = Object.entries(byCategory).filter(([, t]) => t !== other);
    if (rest.length) plural = true;
    compact.push([String(z), other, ...rest.map(([c, t]) => c + t).sort()]);
  }
  const goesOn = (a, b) => a.length && b.length && +b[0] === +a[0] + 1 && same(a.slice(1), b.slice(1));
  while (compact.length > 1 && goesOn(compact.at(-2), compact.at(-1))) compact.pop();
  // The plural categories of the shown numbers: integers 0–199 (above, an
  // integer's is that of 100 + n % 100), and 1.0–9.9 by tenths.
  let plurals = [];
  if (plural) {
    let whole = "", tenths = "";
    for (let n = 0; n < 200; n++) whole += CATEGORY[pr.select(n)];
    for (let n = 10; n < 100; n++) tenths += CATEGORY[pr.select(n / 10)];
    for (let n = 200; n < 1000000; n += 97) if (CATEGORY[pr.select(n)] !== whole[100 + (n % 100)]) fail(`plural of ${n}`);
    plurals = [whole, tenths];
  }
  // Dates and times: the Gregorian calendar at UTC, of the wall time.
  const dtf = (o, tag = l) => new Intl.DateTimeFormat(`${tag}${tag.includes("-u-") ? "" : "-u"}-ca-gregory`, { timeZone: "UTC", ...o });
  const date = (o) => {
    const f = dtf(o);
    let out = "", names = [];
    for (const p of f.formatToParts(UTC(2026, 9, 5, 13, 2))) {
      const v = toAscii(p.value);
      if (p.type === "literal") out += literal(p.value);
      else if (p.type === "year") out += v === "2026" ? "{y}" : v === "26" ? "{yy}" : fail(`year ${v}`);
      else if (p.type === "day") out += v === "5" ? "{d}" : v === "05" ? "{dd}" : fail(`day ${v}`);
      else if (p.type === "month" && (v === "9" || v === "09")) out += v === "9" ? "{M}" : "{MM}";
      else if (p.type === "month") {
        out += "{N}";
        for (let m = 1; m <= 12; m++) names.push(literal(f.formatToParts(UTC(2026, m, 5)).find((q) => q.type === "month").value));
      } else fail(`date part ${p.type}`);
    }
    return [out, names];
  };
  const time = (tag) => {
    const f = dtf({ timeStyle: "short" }, tag);
    const at = (m) => f.formatToParts(UTC(2026, 9, 5, (m / 60) | 0, m % 60));
    const hour = (h) => toAscii(at(h * 60 + 2).find((p) => p.type === "hour").value);
    const hours = `${hour(0)}/${hour(1)}/${hour(13)}`;
    const field = { "12/1/1": "h", "12/01/01": "hh", "0/1/13": "H", "00/01/13": "HH", "0/1/1": "K", "00/01/01": "KK", "24/1/13": "k", "24/01/13": "kk" }[hours]
      ?? fail(`hours ${hours}`);
    let out = "", periods = [];
    for (const p of at(13 * 60 + 2)) {
      const v = toAscii(p.value);
      if (p.type === "literal") out += literal(p.value);
      else if (p.type === "hour") out += `{${field}}`;
      else if (p.type === "minute") out += v === "02" ? "{mm}" : v === "2" ? "{m}" : fail(`minute ${v}`);
      else if (p.type === "dayPeriod") out += "{a}";
      else fail(`time part ${p.type}`);
    }
    if (out.includes("{a}")) {
      // CLDR's day periods mostly begin on the hour: an hour whose first,
      // middle and last minute say one name is read once, another (`hi-Latn`'s
      // noon) minute by minute.
      const name = (m) => at(m).find((p) => p.type === "dayPeriod").value;
      let last = null;
      for (let h = 0; h < 24; h++) {
        const v = name(h * 60);
        const whole = name(h * 60 + 30) === v && name(h * 60 + 59) === v;
        for (let m = h * 60; m < (whole ? h * 60 + 1 : h * 60 + 60); m++) {
          const w = m === h * 60 ? v : name(m);
          if (w !== last) periods.push([String(m), literal((last = w))]);
        }
      }
    }
    return [out, periods];
  };
  const wf = dtf({ weekday: "long" });
  const at = UTC(2026, 9, 5, 13, 2);
  const both = dtf({ dateStyle: "medium", timeStyle: "short" }).format(at);
  const [d, t] = [dtf({ dateStyle: "medium" }).format(at), dtf({ timeStyle: "short" }).format(at)];
  if (both.split(d).length !== 2 || both.split(t).length !== 2) fail(`a date and time ${both}`);
  const rtf = new Intl.RelativeTimeFormat(l, { numeric: "auto" });
  const title = (w) => { const [c, ...rest] = w; return c.toLocaleUpperCase(l) + rest.join(""); };
  const [medium, monthYear, short] = [date({ dateStyle: "medium" }), date({ month: "long", year: "numeric" }), date({ dateStyle: "short" })];
  const [plainTime, h12, h23] = [time(l), time(`${l}-u-hc-h12`), time(`${l}-u-hc-h23`)];
  return {
    digits: ascii ? "" : native.join(""),
    decimal: sym("decimal"),
    group: sym("group"),
    grouping: grouping(nf),
    decimalStyle: [template(parts(nf, 1234.5)), template(parts(nf, -1234.5))],
    // A percentage, too, has its own symbols and grouping (`rhg-BD` groups it by threes).
    percent: [template(parts(pf, 0.26)), template(parts(pf, -0.26)),
      literal(parts(new Intl.NumberFormat(l, { style: "percent", minimumFractionDigits: 1 }), 0.015).find((p) => p.type === "decimal").value),
      literal(parts(pf, 12345678901.23).find((p) => p.type === "group")?.value ?? ""), grouping(pf, 100)],
    currencyTemplates,
    currencies,
    compact,
    plurals,
    medium: medium[0], mediumNames: medium[1],
    monthYear: monthYear[0], monthYearNames: monthYear[1],
    short: short[0], shortNames: short[1],
    weekdays: [0, 1, 2, 3, 4, 5, 6].map((i) => literal(wf.format(UTC(2026, 10, 4 + i)))),
    time: plainTime[0], timePeriods: plainTime[1],
    time12: h12[0], time12Periods: h12[1],
    time23: h23[0], time23Periods: h23[1],
    dateTime: literal(both.replace(d, "\u0000").replace(t, "\u0001")).replace("\u0000", "{D}").replace("\u0001", "{T}"),
    days: [0, -1, 1].map((n) => literal(title(rtf.format(n, "day")))),
  };
}

if (process.argv.includes("--oracle")) {
  oracle();
  process.exit(0);
}

// JavaScriptCore frees ICU's memory behind a formatter or its parts late, so
// the records are made 10 locales to a process.
const at = process.argv.indexOf("--records");
if (at >= 0) {
  const out = { full: {}, refused: [] };
  for (const l of process.argv[at + 1].split(",")) {
    try { out.full[l] = record(l); } catch (e) { out.refused.push(e.message); }
    Bun.gc(true);
  }
  process.stdout.write(JSON.stringify(out));
  process.exit(0);
}
const run = (...args) => {
  const child = Bun.spawnSync([process.execPath, fileURLToPath(import.meta.url), ...args], { stderr: "inherit" });
  if (!child.success) throw new Error(`formats.mjs ${args[0]} failed`);
  return JSON.parse(child.stdout.toString());
};
const listed = locales();
const full = {}, refused = [];
for (let i = 0; i < listed.length; i += 10) {
  const batch = run("--records", listed.slice(i, i + 10).join(","));
  Object.assign(full, batch.full);
  refused.push(...batch.refused);
}
// Each locale keeps only the fields its nearest listed parent (the tag less
// its last subtag) does not have, as CLDR's inheritance does.
const parent = (l) => {
  for (let p = l.split("-"); p.length > 1;) {
    p.pop();
    if (full[p.join("-")]) return p.join("-");
  }
  return null;
};
// A list as long as the parent's keeps only the items that differ, by index.
const diff = (v, was) => {
  if (!Array.isArray(v) || !Array.isArray(was) || v.length !== was.length || !v.length) return v;
  return Object.fromEntries(v.flatMap((x, i) => (same(x, was[i]) ? [] : [[i, x]])));
};
const out = {};
for (const [l, r] of Object.entries(full)) {
  const p = parent(l);
  out[l] = p ? Object.fromEntries(Object.entries(r).flatMap(([k, v]) => (same(v, full[p][k]) ? [] : [[k, diff(v, full[p][k])]]))) : r;
}
writeFileSync(OUT, JSON.stringify({
  _about: "Written by formats.mjs from Bun 1.4.2's Intl (ICU 78.1, CLDR 48); do not edit. A locale lists the fields its parent (the tag less its last subtag) lacks; of a list as long as the parent's, only the items that differ, by index.",
  refused,
  locales: out,
}, null, 0).replace(/,"(?=[a-z]{2,3}(-[A-Za-z0-9]+)*":)/g, ',\n"') + "\n");
console.error(`${Object.keys(full).length} locales, ${refused.length} refused`);

/** The formatters' outputs in a set of locales, from Intl itself (formatDay's
 * relative words and their joining are Foundation's convention, LLP 1116.020
 * D8: the word, title-cased, in the medium date's place). */
function oracle() {
  // de-AT, en-150, en-BG and rhg-BD set money's symbols or grouping apart
  // from a number's; de-u-nu-latn and en-u-hc-h23 are tables named with a
  // Unicode extension, the second forcing its hour cycle.
  const LOCALES = ["en", "de", "fr", "ja", "ar", "ar-EG", "hi", "pt-BR", "en-GB", "de-CH", "es", "ru", "pl", "zh", "ko", "fa", "he", "sv", "fi", "mn", "my", "vi", "tr",
    "de-AT", "en-150", "en-BG", "rhg-BD", "de-u-nu-latn", "en-u-hc-h23"];
  const INSTANTS = [
    [0, 0], [-0.5, 60], [1790000000000.7, 345], [1790000000000, -720], [1791762120000, -420], [43200000, 0],
    [45000000, 0], [1800000, 0], [-62135596800000, 0], [253402300799999, 0], [951782400000, 0], [-1790000000000.7, 840],
    [1767225540000, 330], [1775000000000, 0], [1785000000000, 120], [1757000000000, -300],
  ];
  const NUMBERS = [0, -0, 0.5, 1, 1.005, 2.5, 12.3456, 999.9995, 1234.5, -1234.5, 12345, 123456.789, 1234567, -9876543.21, 1e9, 0.0042, 1e21, 26, -0.001];
  const COMPACT = [0, -0, 0.29, 1, 999, 1000, 1250, 1999, 2000, 3000, 11000, 12345, 100000, 101000, 999999, 1234567, -1234567, 1e9, 2.5e9, 1e12, 1e15, 1.234e16, 3.5e19, 1e20, 4.56e22, 0.0012];
  const wallDate = (ms, off) => new Date(Math.trunc(ms) + off * 60000);
  const rows = [];
  for (const l of LOCALES) {
    const dtf = (o, tag = l) => new Intl.DateTimeFormat(`${tag}${tag.includes("-u-") ? "" : "-u"}-ca-gregory`, { timeZone: "UTC", ...o });
    for (const [ms, off] of INSTANTS) {
      const d = wallDate(ms, off);
      // The format slot's spelling (`format_tag`): a forced cycle after a space, none when the table's name forces one.
      for (const hc of l.includes("-hc-") ? [""] : ["", "h12", "h23"]) {
        const tag = !hc ? l : l.includes("-u-") ? `${l}-hc-${hc}` : `${l}-u-hc-${hc}`;
        rows.push([hc ? `${l} ${hc}` : l, "formatTime", [ms, off, "short"], dtf({ timeStyle: "short" }, tag).format(d)]);
      }
      rows.push([l, "formatDate", [ms, off, "medium"], dtf({ dateStyle: "medium" }).format(d)]);
      rows.push([l, "formatDate", [ms, off, "month-year"], dtf({ month: "long", year: "numeric" }).format(d)]);
    }
    const rtf = new Intl.RelativeTimeFormat(l, { numeric: "auto" });
    const word = (n) => { const [c, ...rest] = rtf.format(n, "day"); return c.toLocaleUpperCase(l) + rest.join(""); };
    const now = 1791762120000, off = -420, dayMs = 86400000;
    for (const back of [0, 0.1, 1, 1.3, 2, 5, 6, 7, 30, -1, -2, -400]) {
      const then = now - back * dayMs;
      const wall = (x) => Math.floor((Math.trunc(x) + off * 60000) / dayMs);
      const apart = wall(then) - wall(now);
      const d = wallDate(then, off);
      const time = dtf({ timeStyle: "short" }).format(d);
      const relative = { 0: word(0), [-1]: word(-1), 1: word(1) }[apart];
      const both = dtf({ dateStyle: "medium", timeStyle: "short" }).format(d);
      const dayLabel = relative ?? dtf({ dateStyle: "medium" }).format(d);
      const dayTime = relative === undefined ? both : both.replace(dtf({ dateStyle: "medium" }).format(d), relative);
      const list = apart === 0 ? time : apart === -1 ? word(-1) : apart >= -6 && apart <= -2 ? dtf({ weekday: "long" }).format(d) : dtf({ dateStyle: "short" }).format(d);
      rows.push([l, "formatDay", [then, now, off, "day"], dayLabel]);
      rows.push([l, "formatDay", [then, now, off, "day-time"], dayTime]);
      rows.push([l, "formatDay", [then, now, off, "list"], list]);
    }
    const nf = (o) => new Intl.NumberFormat(l, o);
    const z = (n) => (Object.is(n, -0) ? "-0" : n);
    for (const n of NUMBERS) {
      rows.push([l, "formatNumber", [z(n), "decimal", ""], nf({}).format(n)]);
      rows.push([l, "formatNumber", [z(n), "percent", ""], nf({ style: "percent" }).format(n)]);
      for (const code of ["USD", "EUR", "JPY", "CHF"]) rows.push([l, "formatNumber", [z(n), "currency", code], nf({ style: "currency", currency: code }).format(n)]);
    }
    for (const code of Object.keys(CODES)) rows.push([l, "formatNumber", [-1234.5, "currency", code], nf({ style: "currency", currency: code }).format(-1234.5)]);
    for (const n of COMPACT) rows.push([l, "formatNumber", [z(n), "compact", ""], nf({ notation: "compact", roundingMode: "trunc", signDisplay: "negative" }).format(n)]);
  }
  // Each locale's record as the compiler bakes it (contract/lower/src/formats.rs `record`), which the JS target's
  // test reads and the runner's test holds to the compiler's.
  const listed = JSON.parse(readFileSync(OUT, "utf8")).locales;
  const find = (tag) => { for (let t = tag; ; t = t.slice(0, t.lastIndexOf("-"))) { const k = Object.keys(listed).find((l) => l.toLowerCase() === t.toLowerCase()); if (k) return k; if (!t.includes("-")) return null; } };
  const fields = (name) => {
    const at = name.lastIndexOf("-"), parent = at < 0 ? null : find(name.slice(0, at)), merged = parent ? fields(parent) : {};
    for (const [k, v] of Object.entries(listed[name])) {
      if (Array.isArray(merged[k]) && v && typeof v === "object" && !Array.isArray(v)) { merged[k] = [...merged[k]]; for (const [i, x] of Object.entries(v)) merged[k][+i] = x; }
      else merged[k] = v;
    }
    return merged;
  };
  const join = (v, depth) => (Array.isArray(v) ? v.map((x) => join(x, depth + 1)).join(depth ? "\u001d" : "\u001f") : v);
  const order = Object.keys(record("en"));
  const records = Object.fromEntries(LOCALES.map((l) => { const f = fields(find(l)); return [l, order.map((k) => join(f[k], 0)).join("\u001e")]; }));
  const out = resolve(dirname(OUT), "../../../runner/tests/it/format_locales.json");
  writeFileSync(out, `{"_about":"Written by contract/lower/cldr/formats.mjs --oracle from Bun 1.4.2's Intl (ICU 78.1); do not edit. Each row: the format locale, the entry, its arguments, Intl's string. Each record: a table's format data as the compiler bakes it.",\n"records":${JSON.stringify(records)},\n"rows":[\n${rows.map((r) => JSON.stringify(r)).join(",\n")}\n]}\n`);
  console.error(`${rows.length} rows`);
}
