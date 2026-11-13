//! The number and date data the formatters read, baked for the locales an
//! app ships strings tables for (LLP 1054.000.003 D6, amended 2026-11-12).
//!
//! `cldr/formats.json` is `Intl`'s, written by `cldr/formats.mjs` from the
//! pinned Bun (ICU 78.1, CLDR 48). It is a compiler dependency only, as the
//! script directions are (LLP 1060 D4): a plan carries the record of each
//! table, and a host carries none.
//!
//! A record is one string, its fields in [`FIELDS`] order and separated by
//! U+001E; a list's items by U+001F, and an item's parts by U+001D. The
//! runner's `format::locale` and the JS target's `format-locale.js` read it. In a
//! template, `{#}` is a number's digits, `{¤}` its currency symbol, `{D}`
//! and `{T}` a date and a time; `{y}` the year, `{yy}` its last two digits,
//! `{M}` `{MM}` the month, `{N}` its name, `{d}` `{dd}` the day, `{h}` `{hh}`
//! (1–12), `{H}` `{HH}` (0–23), `{K}` `{KK}` (0–11), `{k}` `{kk}` (1–24) the
//! hour, `{m}` `{mm}` the minute and `{a}` the day period. Other text is
//! literal.
//!
//! | field | holds |
//! |---|---|
//! | `digits` | the ten digits, `""` for ASCII |
//! | `decimal`, `group` | the symbols |
//! | `grouping` | three digits: the primary and secondary group sizes, the fewest integer digits grouped |
//! | `decimalStyle` | a positive and a negative template |
//! | `percent` | those, then a percentage's decimal and group symbols and grouping (as `grouping`) |
//! | `currencyTemplates` | such pairs, by index |
//! | `currencies` | the roster's 24 codes in its order: a template index, the symbol, and an amount's decimal and group symbols and grouping (as `grouping`; the group `""` when it is not grouped) |
//! | `compact` | from 10⁰ to the last magnitude with a pattern of its own (a larger number takes the last): none, or the integer digits shown, the template, and each plural category's own that differs, after its letter (`f{#} آلاف`) |
//! | `plurals` | none, or the plural category letters of 0–199 and of 1.0–9.9 by tenths |
//! | `medium`, `monthYear`, `short` | a date template, then its month names |
//! | `weekdays` | Sunday's first |
//! | `time`, `time12`, `time23` | a time template, then its day periods: their first minute and their name |
//! | `dateTime` | the medium date with the short time |
//! | `days` | today, yesterday and tomorrow, in a label's case |

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The record's fields, in order.
pub const FIELDS: [&str; 25] = [
    "digits",
    "decimal",
    "group",
    "grouping",
    "decimalStyle",
    "percent",
    "currencyTemplates",
    "currencies",
    "compact",
    "plurals",
    "medium",
    "mediumNames",
    "monthYear",
    "monthYearNames",
    "short",
    "shortNames",
    "weekdays",
    "time",
    "timePeriods",
    "time12",
    "time12Periods",
    "time23",
    "time23Periods",
    "dateTime",
    "days",
];

type Locales = BTreeMap<String, serde_json::Map<String, serde_json::Value>>;

fn locales() -> &'static Locales {
    static LOCALES: OnceLock<Locales> = OnceLock::new();
    LOCALES.get_or_init(|| {
        let text = include_str!("../cldr/formats.json");
        let mut file: serde_json::Value = serde_json::from_str(text).expect("formats.json parses");
        serde_json::from_value(file["locales"].take()).expect("formats.json's locales")
    })
}

/// The listed locale a table's tag formats as: RFC 4647 lookup without
/// case (`de-CH-x`, `de-CH`, `de`), as a table is chosen for a viewer.
fn lookup(tag: &str) -> Option<&'static str> {
    let tag = tag.split("-u-").next().unwrap_or(tag);
    let mut tag = tag;
    loop {
        if let Some(name) = locales().keys().find(|l| l.eq_ignore_ascii_case(tag)) {
            return Some(name);
        }
        tag = &tag[..tag.rfind('-')?];
    }
}

/// A listed locale's fields, its parents' (the tag less its last subtag)
/// under its own; a list of the parent's length is patched by index.
fn fields(name: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut merged = match name.rfind('-') {
        Some(at) => match lookup(&name[..at]) {
            Some(parent) => fields(parent),
            None => serde_json::Map::new(),
        },
        None => serde_json::Map::new(),
    };
    for (key, value) in &locales()[name] {
        match (merged.get_mut(key), value) {
            (Some(serde_json::Value::Array(items)), serde_json::Value::Object(patch)) => {
                for (i, item) in patch {
                    let i: usize = i.parse().expect("an index");
                    items[i] = item.clone();
                }
            }
            _ => {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    merged
}

fn join(value: &serde_json::Value, depth: usize, out: &mut String) {
    match value {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Array(items) => {
            let separator = ['\u{1f}', '\u{1d}'][depth.min(1)];
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(separator);
                }
                join(item, depth + 1, out);
            }
        }
        other => panic!("formats.json holds {other}"),
    }
}

/// The record a table named `tag` formats with: its locale's, or `en`'s
/// when CLDR lists none of its prefixes.
pub fn record(tag: &str) -> String {
    let fields = fields(lookup(tag).unwrap_or("en"));
    let mut out = String::new();
    for (i, name) in FIELDS.iter().enumerate() {
        if i > 0 {
            out.push('\u{1e}');
        }
        join(&fields[*name], 0, &mut out);
    }
    out
}

/// Each table's record, in `tables` order, when some table formats
/// otherwise than `en`, which the runner's own `en-US` forms print
/// (LLP 1054.000.003 D6); `None` when none does.
pub fn records<'a>(tables: impl IntoIterator<Item = &'a str>) -> Option<Vec<String>> {
    let en = record("en");
    let all: Vec<String> = tables.into_iter().map(record).collect();
    all.iter().any(|r| *r != en).then_some(all)
}
