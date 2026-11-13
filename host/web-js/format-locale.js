// The formatters in a locale other than en-US on the JS target (LLP 1054.000.003 D6): runner/src/format/locale.rs's
// rules over the same record (contract/lower/src/formats.rs has its fields), byte for byte. Imported only by a plan
// whose tables carry format data (emit.rs), which hands it the records with `formats`.
import { localized, wall, dateWall, civil, rounded, shortest, x_formatDate, CODES } from "./format.js";
import { memo } from "./rt.js";
import { reportPlace } from "./navigation.js";

const RS = "\u001e", US = "\u001f", GS = "\u001d";
const F = { digits: 0, decimal: 1, group: 2, grouping: 3, decimalStyle: 4, percent: 5, currencyTemplates: 6, currencies: 7, compact: 8,
  plurals: 9, medium: 10, mediumNames: 11, monthYear: 12, monthYearNames: 13, short: 14, shortNames: 15, weekdays: 16, time: 17,
  timePeriods: 18, time12: 19, time12Periods: 20, time23: 21, time23Periods: 22, dateTime: 23, days: 24 };
let Records = new Map();

/** The plan's tables' records, [name, record], and the hook format.js calls with a format locale other than "". */
export function formats(rows) {
  Records = new Map(rows.map(([name, record]) => [name, record.split(RS)]));
  localized(format);
}
/** `format_tag` (plan/src/strings.rs): the table's whole name, then, after a space, the hour cycle the viewer's format
 * locale forces (h12 or h23) unless the name forces one: `de h23`, `de-u-ca-gregory`. */
export function formatTag(table, formatLocale) {
  const c = tagCycle(table) ? null : tagCycle(formatLocale);
  return c ? `${table} ${c}` : table;
}
const tagCycle = (tag) => (tag.includes("-u-") ? cycle(tag.slice(tag.indexOf("-u-") + 3)) : null);
/** The format slot (LLP 1054.000.003 D6): the locale slot's table with the hour cycle the page's format locale forces
 * (runner/src/runner/time.rs `format_locale_of`). A render has no place: the table alone. */
export function formatSlot(locale) {
  let place = "";
  if (!globalThis.__exactRender) try { const [l, , , f] = reportPlace().split("\0"); place = f || l || ""; } catch {}
  return memo(() => formatTag(locale(), place));
}
function cycle(extension) {
  const s = extension.split("-");
  const i = s.findIndex((k) => k.toLowerCase() === "hc");
  const c = i < 0 ? "" : (s[i + 1] ?? "").toLowerCase();
  return c === "h12" || c === "h23" ? c : null;
}

const item = (r, f, i) => r[f].split(US)[i] ?? "";
const signed = (pair, negative) => pair.split(/[\u001f\u001d]/)[negative ? 1 : 0] ?? "{#}";
/** ASCII digits in the locale's own. */
const native = (r, s) => (r[F.digits] ? s.replace(/[0-9]/g, (d) => Array.from(r[F.digits])[d]) : s);
/** An integer's digits and fraction digits with the locale's symbols and grouping (locale.rs `body`). */
function body(r, int, frac, least, symbols = [r[F.decimal], r[F.group], r[F.grouping]]) {
  const [decimal, group, g] = symbols, size = (i) => (g[i] === undefined ? 3 : +g[i]), primary = size(0), secondary = size(1);
  least = Math.max(least, size(2));
  const grouped = int.length >= primary + least;
  let s = "";
  for (let i = 0; i < int.length; i++) {
    const left = int.length - i;
    if (i > 0 && grouped && left >= primary && (left - primary) % secondary === 0) s += "\u0000";
    s += int[i];
  }
  if (frac.length) s += "\u0001" + frac;
  return native(r, s).replaceAll("\u0000", group).replaceAll("\u0001", decimal);
}

/** A percentage and an amount of money take their own symbols and grouping (locale.rs `styled`). */
function styled(r, n, style, code) {
  let pair, symbol = "", min = 0, max = 3, shift = 0, money;
  if (style === "decimal") pair = r[F.decimalStyle];
  else if (style === "percent") { pair = r[F.percent]; max = 0; shift = 2; money = pair.split(US).slice(2); }
  else if (style === "currency") {
    const i = Object.keys(CODES).indexOf(code);
    if (i < 0) return null;
    min = max = CODES[code];
    const [t, s, ...symbols] = item(r, F.currencies, i).split(GS);
    symbol = s; pair = item(r, F.currencyTemplates, +t); money = symbols;
  } else return null;
  if (!Number.isFinite(n)) return "";
  const [int, frac] = rounded(n, shift, min, max);
  return signed(pair, n < 0 || Object.is(n, -0)).replace("{#}", body(r, int, frac, 1, money)).replace("{¤}", symbol);
}

function category(r, int, kept) {
  const [whole = "", tenths = ""] = r[F.plurals].split(US);
  const letter = kept ? tenths[+int.slice(-1) * 10 + +kept[0] - 10] : (() => {
    const last = +int.slice(-3);
    return whole[int.length <= 3 && last < 200 ? last : 100 + (last % 100)];
  })();
  return letter ?? "x";
}

function compact(r, n) {
  if (!Number.isFinite(n)) return "";
  let [int, frac] = shortest(Math.abs(n));
  const entries = r[F.compact].split(US);
  const m = int.length ? Math.min(int.length - 1, entries.length - 1) : null;
  const parts = (m === null ? "" : entries[m]).split(GS);
  const shown = parts[0] === "" ? null : +parts[0];
  if (shown !== null) {
    const cut = int.length - Math.max(m + 1 - shown, 0);
    frac = int.slice(cut) + frac;
    int = int.slice(0, cut);
  }
  let kept = "";
  if (!int.length) { let first = 0; while (first < frac.length && frac[first] === "0") first++; kept = frac.slice(0, first + 2); }
  else if (int.length === 1) kept = frac.slice(0, 1);
  kept = kept.replace(/0+$/, "");
  if (!int) int = "0";
  let template = "{#}";
  if (shown !== null) {
    const c = category(r, int, kept);
    template = parts.slice(2).find((p) => p[0] === c)?.slice(1) ?? parts[1];
  }
  const positive = template.replace("{#}", body(r, int, kept, 2));
  return n < 0 ? signed(r[F.decimalStyle], true).replace("{#}", positive) : positive;
}

/** A wall time's `{…}` fields filled into `template` (locale.rs `fill`). */
function fill(r, template, w, names, periods) {
  const [year, month, day] = civil(Math.floor(w / 864e5));
  const minutes = Math.floor((((w % 864e5) + 864e5) % 864e5) / 6e4), hour = (minutes / 60) | 0, minute = minutes % 60;
  const num = (n, width) => native(r, String(n).padStart(width, "0"));
  return template.replace(/\{([^}]*)\}/g, (whole, f) => {
    switch (f) {
      case "y": return num(year, 1);
      case "yy": return num(((year % 100) + 100) % 100, 2);
      case "M": case "MM": return num(month, f.length);
      case "d": case "dd": return num(day, f.length);
      case "h": case "hh": return num(((hour + 11) % 12) + 1, f.length);
      case "H": case "HH": return num(hour, f.length);
      case "K": case "KK": return num(hour % 12, f.length);
      case "k": case "kk": return num(((hour + 23) % 24) + 1, f.length);
      case "m": case "mm": return num(minute, f.length);
      case "N": return names.split(US)[month - 1] ?? "";
      case "a": {
        let name = "";
        for (const p of periods.split(US)) { const [from, n] = p.split(GS); if (n !== undefined && +from <= minutes) name = n; else break; }
        return name;
      }
      default: return whole;
    }
  });
}
const date = (r, w, t, names) => fill(r, r[t], w, r[names], "");
function time(r, w, c) {
  const [t, p] = c === "h12" ? [F.time12, F.time12Periods] : c === "h23" ? [F.time23, F.time23Periods] : [F.time, F.timePeriods];
  return fill(r, r[t], w, "", r[p]);
}
function day(r, then, now, off, mode, c) {
  const w = wall(then, off), t = wall(now, off);
  if (w === null || t === null) return "";
  const days = Math.floor(w / 864e5), apart = days - Math.floor(t / 864e5);
  if (mode === "list") {
    if (apart === 0) return time(r, w, c);
    if (apart === -1) return item(r, F.days, 1);
    if (apart >= -6 && apart <= -2) return item(r, F.weekdays, (((days + 4) % 7) + 7) % 7);
    return date(r, w, F.short, F.shortNames);
  }
  const label = apart === 0 ? item(r, F.days, 0) : apart === -1 ? item(r, F.days, 1) : apart === 1 ? item(r, F.days, 2) : date(r, w, F.medium, F.mediumNames);
  return mode === "day" ? label : r[F.dateTime].replace("{T}", time(r, w, c)).replace("{D}", label);
}

/** The entry `name` of `args` (its parameters, the format locale last) in the format locale `loc`. */
function format(name, args, loc) {
  // `Plan::format_data`: the table before the space, the forced cycle after it, else the name's own.
  const at = loc.indexOf(" "), table = at < 0 ? loc : loc.slice(0, at), forced = at < 0 ? undefined : loc.slice(at + 1), r = Records.get(table);
  if (!r) throw new Error(`${name}: no format data for ${loc}`);
  const c = forced === undefined ? tagCycle(table) : forced === "h12" || forced === "h23" ? forced : null;
  switch (name) {
    case "formatTime": { const w = wall(args[0], args[1]); return w === null ? "" : time(r, w, c); }
    case "formatDate": {
      if (args[2] === "iso") return x_formatDate(args[0], args[1], "iso", "");
      const w = dateWall(args[0], args[1]);
      return w === null ? "" : args[2] === "month-year" ? date(r, w, F.monthYear, F.monthYearNames) : date(r, w, F.medium, F.mediumNames);
    }
    case "formatDay": return day(r, args[0], args[1], args[2], args[3], c);
    case "formatNumber": return args[1] === "compact" ? compact(r, args[0]) : styled(r, args[0], args[1], args[2]);
  }
}
