//! The formatters in a locale other than `en-US`, from the format data the
//! compiler bakes for each strings table (LLP 1054.000.003 D6, amended
//! 2026-11-12). The record's fields and template syntax are documented in
//! `contract/lower/src/formats.rs`; the JS target's `format-locale.js` reads the
//! same record the same way. Each entry's rules are the `en-US` ones of
//! [`super`], with the locale's symbols, digits, grouping, templates and
//! names.

use super::{js_digits_shortest, rounded, CURRENCIES};
use crate::stdlib::wall_ms;
use exact_num::date::civil;
use exact_plan::{Stdlib, Value};

/// The record's fields, in `contract_lower::formats::FIELDS`' order.
#[derive(Clone, Copy)]
enum Field {
    Digits,
    Decimal,
    Group,
    Grouping,
    DecimalStyle,
    Percent,
    CurrencyTemplates,
    Currencies,
    Compact,
    Plurals,
    Medium,
    MediumNames,
    MonthYear,
    MonthYearNames,
    Short,
    ShortNames,
    Weekdays,
    Time,
    TimePeriods,
    Time12,
    Time12Periods,
    Time23,
    Time23Periods,
    DateTime,
    Days,
}

struct Record<'a>(Vec<&'a str>);

impl<'a> Record<'a> {
    fn get(&self, f: Field) -> &'a str {
        self.0.get(f as usize).copied().unwrap_or("")
    }

    fn item(&self, f: Field, i: usize) -> &'a str {
        self.get(f).split('\u{1f}').nth(i).unwrap_or("")
    }

    /// `n`'s decimal digits in the locale's own.
    fn push_digits(&self, ascii: &str, out: &mut String) {
        let digits = self.get(Field::Digits);
        if digits.is_empty() {
            out.push_str(ascii);
            return;
        }
        for c in ascii.chars() {
            match c.to_digit(10) {
                Some(d) => out.extend(digits.chars().nth(d as usize)),
                None => out.push(c),
            }
        }
    }

    /// An integer and its fraction digits with the locale's symbols and
    /// grouping: groups of the primary size, then the secondary, when the
    /// integer has at least the primary size and `least` more digits.
    fn body(&self, int: &[u8], frac: &[u8], least: usize) -> String {
        let symbols = Symbols {
            decimal: self.get(Field::Decimal),
            group: self.get(Field::Group),
            grouping: self.get(Field::Grouping),
        };
        self.body_in(int, frac, least, &symbols)
    }

    /// [`Record::body`] with other symbols and grouping (an amount of money's).
    fn body_in(&self, int: &[u8], frac: &[u8], least: usize, symbols: &Symbols<'_>) -> String {
        let grouping = symbols.grouping.as_bytes();
        let size = |i: usize| grouping.get(i).map_or(3, |b| usize::from(b - b'0'));
        let (primary, secondary) = (size(0), size(1));
        let least = least.max(size(2));
        let mut ascii = String::with_capacity(int.len() + frac.len() + 8);
        let grouped = int.len() >= primary + least;
        for (i, &d) in int.iter().enumerate() {
            let left = int.len() - i;
            if i > 0 && grouped && left >= primary && (left - primary).is_multiple_of(secondary) {
                ascii.push('\u{0}');
            }
            ascii.push(char::from(d));
        }
        if !frac.is_empty() {
            ascii.push('\u{1}');
            ascii.extend(frac.iter().map(|&d| char::from(d)));
        }
        let mut out = String::with_capacity(ascii.len() * 2);
        self.push_digits(&ascii, &mut out);
        out.replace('\u{0}', symbols.group)
            .replace('\u{1}', symbols.decimal)
    }

    /// One of a pair of templates (positive, negative).
    fn signed(&self, pair: &'a str, negative: bool) -> &'a str {
        pair.split(['\u{1f}', '\u{1d}'])
            .nth(usize::from(negative))
            .unwrap_or("{#}")
    }
}

/// A localized entry's value from the record `data`, in the hour cycle
/// `cycle` forces; `None` where [`super::formatting`] would refuse.
pub(super) fn formatting(
    f: Stdlib,
    args: &[Value],
    data: &str,
    cycle: Option<&str>,
) -> Option<Value> {
    let r = Record(data.split('\u{1e}').collect());
    let num = |i: usize| args.get(i).and_then(Value::as_number);
    let style = |i: usize| args.get(i).and_then(Value::as_str);
    let text = match f {
        Stdlib::FormatTime => match style(2)? {
            "short" => match wall_ms(num(0)?, num(1)?) {
                Some(wall) => time(&r, wall, cycle),
                None => String::new(),
            },
            _ => return None,
        },
        Stdlib::FormatDate => {
            let style = style(2)?;
            let wall = super::date_wall(args.first()?, num(1)?);
            match (style, wall) {
                (_, None) if matches!(style, "medium" | "month-year" | "iso") => String::new(),
                ("medium", Some(wall)) => date(&r, wall, Field::Medium, Field::MediumNames),
                ("month-year", Some(wall)) => {
                    date(&r, wall, Field::MonthYear, Field::MonthYearNames)
                }
                // `toISOString`'s, in every locale.
                ("iso", Some(wall)) => super::date(wall, super::Style::Iso),
                _ => return None,
            }
        }
        Stdlib::FormatDay => {
            let mode = style(3)?;
            if !matches!(mode, "day" | "day-time" | "list") {
                return None;
            }
            day(&r, num(0)?, num(1)?, num(2)?, mode, cycle)
        }
        Stdlib::FormatNumber => {
            let n = num(0)?;
            match style(1)? {
                "compact" => compact(&r, n),
                other => styled(&r, n, other, style(2).unwrap_or(""))?,
            }
        }
        _ => return None,
    };
    Some(Value::str(&text))
}

/// A number's decimal and group symbols and its grouping (the record's
/// `grouping` field's three digits).
struct Symbols<'a> {
    decimal: &'a str,
    group: &'a str,
    grouping: &'a str,
}

/// `decimal`, `percent` and `currency`: [`super::styled`]'s digits in the
/// locale's template; a percentage and an amount of money in their own
/// symbols and grouping (`de-AT`'s `€ 1.234,50` beside `1 234,5`).
fn styled(r: &Record<'_>, n: f64, style: &str, code: &str) -> Option<String> {
    let mut own = None;
    let (pair, symbol, min, max, shift) = match style {
        "decimal" => (r.get(Field::DecimalStyle), "", 0, 3, 0),
        "percent" => {
            let mut item = r.get(Field::Percent).split('\u{1f}').skip(2);
            own = Some(Symbols {
                decimal: item.next()?,
                group: item.next()?,
                grouping: item.next()?,
            });
            (r.get(Field::Percent), "", 0, 0, 2)
        }
        "currency" => {
            let i = CURRENCIES.iter().position(|c| c.0 == code)?;
            let digits = CURRENCIES[i].2 as usize;
            let mut item = r.item(Field::Currencies, i).split('\u{1d}');
            let (template, symbol) = (item.next()?, item.next()?);
            own = Some(Symbols {
                decimal: item.next()?,
                group: item.next()?,
                grouping: item.next()?,
            });
            let pair = r.item(Field::CurrencyTemplates, template.parse().ok()?);
            (pair, symbol, digits, digits, 0)
        }
        _ => return None,
    };
    if !n.is_finite() {
        return Some(String::new());
    }
    let (int, frac) = rounded(n, shift, min, max);
    let body = match &own {
        Some(symbols) => r.body_in(&int, &frac, 1, symbols),
        None => r.body(&int, &frac, 1),
    };
    Some(
        r.signed(pair, n.is_sign_negative())
            .replace("{#}", &body)
            .replace("{¤}", symbol),
    )
}

/// `compact`: [`super::compact`]'s rule, with the locale's magnitudes
/// (`万` is 10⁴), plural forms, and grouping of at least two digits more
/// than a group.
fn compact(r: &Record<'_>, n: f64) -> String {
    if !n.is_finite() {
        return String::new();
    }
    let (int, frac) = js_digits_shortest(n.abs());
    // The magnitude's entry; a larger number's is the last one's.
    let entries: Vec<&str> = r.get(Field::Compact).split('\u{1f}').collect();
    let magnitude = int.len().checked_sub(1).map(|m| m.min(entries.len() - 1));
    let entry = magnitude.map_or("", |m| entries[m]);
    let mut parts = entry.split('\u{1d}');
    let shown: Option<usize> = parts.next().and_then(|z| z.parse().ok());
    let (int, frac) = match (shown, magnitude) {
        (Some(z), Some(m)) => {
            let moved = (m + 1).saturating_sub(z);
            let cut = int.len() - moved;
            (int[..cut].to_vec(), [&int[cut..], &frac[..]].concat())
        }
        _ => (int, frac),
    };
    let kept: &[u8] = match int.len() {
        0 => {
            let first = frac.iter().position(|&d| d != b'0').unwrap_or(frac.len());
            &frac[..(first + 2).min(frac.len())]
        }
        1 => &frac[..1.min(frac.len())],
        _ => &[],
    };
    let kept = &kept[..kept.iter().rposition(|&d| d != b'0').map_or(0, |i| i + 1)];
    let int: &[u8] = if int.is_empty() { b"0" } else { &int };
    let body = r.body(int, kept, 2);
    let template = match shown {
        Some(_) => {
            let other = parts.next().unwrap_or("{#}");
            let category = category(r, int, kept);
            parts
                .find_map(|p| p.strip_prefix(category))
                .unwrap_or(other)
        }
        None => "{#}",
    };
    let positive = template.replace("{#}", &body);
    if n < 0.0 {
        r.signed(r.get(Field::DecimalStyle), true)
            .replace("{#}", &positive)
    } else {
        positive
    }
}

/// The plural category letter of a shown compact number (`x` when the
/// record lists none): an integer's by its value (above 199, by 100 plus
/// its last two digits), 1.0–9.9's by tenths.
fn category(r: &Record<'_>, int: &[u8], kept: &[u8]) -> char {
    let mut tables = r.get(Field::Plurals).split('\u{1f}');
    let (whole, tenths) = (tables.next().unwrap_or(""), tables.next().unwrap_or(""));
    let value = |ds: &[u8]| {
        ds.iter()
            .fold(0usize, |v, &d| v * 10 + usize::from(d - b'0'))
    };
    let letter = match kept.first() {
        Some(&d) => tenths
            .as_bytes()
            .get(value(&int[int.len().saturating_sub(1)..]) * 10 + usize::from(d - b'0') - 10),
        None => {
            let last = value(&int[int.len().saturating_sub(3)..]);
            let i = if int.len() <= 3 && last < 200 {
                last
            } else {
                100 + last % 100
            };
            whole.as_bytes().get(i)
        }
    };
    letter.map_or('x', |&b| char::from(b))
}

/// A wall time's `{…}` fields filled into `template`.
fn fill(r: &Record<'_>, template: &str, wall: f64, names: &str, periods: &str) -> String {
    let (year, month, day) = civil((wall / 86_400_000.0).floor() as i64);
    let minutes = (wall.rem_euclid(86_400_000.0) / 60_000.0).floor() as u32;
    let (hour, minute) = (minutes / 60, minutes % 60);
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        let field = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let number = |n: u32, width: usize| format!("{n:0width$}");
        let ascii = match field {
            "y" => number(year as u32, 1),
            "yy" => number(year.rem_euclid(100) as u32, 2),
            "M" | "MM" => number(month, field.len()),
            "d" | "dd" => number(day, field.len()),
            "h" | "hh" => number((hour + 11) % 12 + 1, field.len()),
            "H" | "HH" => number(hour, field.len()),
            "K" | "KK" => number(hour % 12, field.len()),
            "k" | "kk" => number((hour + 23) % 24 + 1, field.len()),
            "m" | "mm" => number(minute, field.len()),
            "N" => {
                out.push_str(names.split('\u{1f}').nth(month as usize - 1).unwrap_or(""));
                continue;
            }
            "a" => {
                let name = periods
                    .split('\u{1f}')
                    .filter_map(|p| p.split_once('\u{1d}'))
                    .take_while(|(from, _)| from.parse::<u32>().is_ok_and(|from| from <= minutes))
                    .last()
                    .map_or("", |(_, name)| name);
                out.push_str(name);
                continue;
            }
            other => {
                out.push('{');
                out.push_str(other);
                out.push('}');
                continue;
            }
        };
        r.push_digits(&ascii, &mut out);
    }
    out.push_str(rest);
    out
}

fn date(r: &Record<'_>, wall: f64, template: Field, names: Field) -> String {
    fill(r, r.get(template), wall, r.get(names), "")
}

/// The short time, in the hour cycle the viewer forced, else the locale's.
fn time(r: &Record<'_>, wall: f64, cycle: Option<&str>) -> String {
    let (template, periods) = match cycle {
        Some("h12") => (Field::Time12, Field::Time12Periods),
        Some("h23") => (Field::Time23, Field::Time23Periods),
        _ => (Field::Time, Field::TimePeriods),
    };
    fill(r, r.get(template), wall, "", r.get(periods))
}

/// [`super::day_label`] with the locale's words, dates, weekdays and the
/// medium date's joiner with the short time.
fn day(
    r: &Record<'_>,
    then: f64,
    now: f64,
    offset: f64,
    mode: &str,
    cycle: Option<&str>,
) -> String {
    let (Some(wall), Some(today)) = (wall_ms(then, offset), wall_ms(now, offset)) else {
        return String::new();
    };
    let day = |w: f64| (w / 86_400_000.0).floor() as i64;
    let (days, apart) = (day(wall), day(wall) - day(today));
    if mode == "list" {
        return match apart {
            0 => time(r, wall, cycle),
            -1 => r.item(Field::Days, 1).to_owned(),
            -6..=-2 => r
                .item(Field::Weekdays, (days + 4).rem_euclid(7) as usize)
                .to_owned(),
            _ => date(r, wall, Field::Short, Field::ShortNames),
        };
    }
    let label = match apart {
        0 => r.item(Field::Days, 0).to_owned(),
        -1 => r.item(Field::Days, 1).to_owned(),
        1 => r.item(Field::Days, 2).to_owned(),
        _ => date(r, wall, Field::Medium, Field::MediumNames),
    };
    if mode == "day" {
        return label;
    }
    r.get(Field::DateTime)
        .replace("{T}", &time(r, wall, cycle))
        .replace("{D}", &label)
}
