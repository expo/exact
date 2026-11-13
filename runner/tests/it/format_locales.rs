//! The formatters in locales other than `en-US` against the web's `Intl`
//! (LLP 1054.000.003 D6, amended 2026-11-12): each row of
//! `format_locales.json` (written by `contract/lower/cldr/formats.mjs
//! --oracle` from Bun 1.4.2, ICU 78.1) through the format data the compiler
//! bakes for a table of that locale, Gregorian dates at a fixed offset.
//! `formatDay`'s relative words take the medium date's place in Intl's
//! `{ dateStyle: "medium", timeStyle: "short" }`, as Foundation's relative
//! formatting puts them.

use exact_plan::builder::PlanBuilder;
use exact_plan::{Plan, Stdlib, TypeKind, Value};
use exact_runner::formatting;

/// A plan whose tables are `tags`, each with its locale's format data.
fn plan(tags: &[&str]) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let init = b.constant(&Value::str(tags[0]));
    let locale = b.slot("#locale", string, init);
    let init = b.constant(&Value::str(tags[0]));
    let format = b.slot("#format", string, init);
    for tag in tags {
        b.locale(tag, false, &contract_lower::formats::record(tag), &[]);
    }
    b.set_locale(locale);
    b.set_format(format);
    b.finish().expect("a plan with format data")
}

fn rows() -> Vec<(String, Stdlib, Vec<Value>, String)> {
    let text = include_str!("format_locales.json");
    let file: serde_json::Value = serde_json::from_str(text).unwrap();
    file["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let args = row[2]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| match a {
                    serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap()),
                    serde_json::Value::String(s) if s == "-0" => Value::Number(-0.0),
                    serde_json::Value::String(s) => Value::str(s),
                    other => panic!("{other}"),
                })
                .collect();
            (
                row[0].as_str().unwrap().to_owned(),
                Stdlib::from_name(row[1].as_str().unwrap()).unwrap(),
                args,
                row[3].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn every_formatter_is_intl_s_in_each_locale() {
    let rows = rows();
    let mut tags: Vec<&str> = rows
        .iter()
        .map(|r| r.0.split(' ').next().unwrap())
        .collect();
    tags.dedup();
    let plan = plan(&tags);
    let mut wrong = Vec::new();
    for (locale, f, mut args, want) in rows.clone() {
        args.push(Value::str(&locale));
        let got = formatting(f, &args, &plan);
        if got != Some(Value::str(&want)) {
            wrong.push(format!(
                "{locale} {}{args:?}: {got:?}, Intl {want:?}",
                f.name()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} rows:\n{}",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );
}

/// The fixture's records, which the JS target's test formats with, are the
/// compiler's.
#[test]
fn the_fixture_s_records_are_the_compiler_s() {
    let file: serde_json::Value =
        serde_json::from_str(include_str!("format_locales.json")).unwrap();
    let records = file["records"].as_object().unwrap();
    assert!(records.len() > 20);
    for (tag, record) in records {
        assert_eq!(
            record.as_str(),
            Some(contract_lower::formats::record(tag).as_str()),
            "{tag}"
        );
    }
}

/// `en`'s data prints what the runner's own `en-US` forms print, row for
/// row, so a plan that bakes it for an `en` table beside another formats
/// English as one with no data does.
#[test]
fn en_s_data_is_the_built_in_en_us() {
    let plan = plan(&["en", "de"]);
    let mut checked = 0;
    for (locale, f, args, _) in rows() {
        if locale != "en" || f == Stdlib::FormatTime {
            continue;
        }
        let mut with = args.clone();
        with.push(Value::str("en"));
        let mut without = args;
        without.push(Value::str(""));
        assert_eq!(
            formatting(f, &with, &plan),
            formatting(f, &without, &plan),
            "{} {with:?}",
            f.name()
        );
        checked += 1;
    }
    assert!(checked > 100);
}

/// A table CLDR lists none of the prefixes of formats as `en`; a table's
/// region and script choose among CLDR's (`de-CH`'s apostrophes, `ar-EG`'s
/// digits); a table is named whole, extension and all; only `h12` and `h23`
/// force an hour cycle, the viewer's after a space, else the name's own.
#[test]
fn a_table_formats_as_its_nearest_listed_locale() {
    let record = contract_lower::formats::record;
    assert_eq!(record("tlh"), record("en"));
    assert_eq!(record("de-AT-x-private"), record("de-AT"));
    assert_ne!(record("de-CH"), record("de"));
    assert_eq!(record("DE-ch"), record("de-CH"));
    assert!(contract_lower::formats::records(["en", "en-US"]).is_none());
    assert!(contract_lower::formats::records(["en", "fr"]).is_some());
    let plan = plan(&["de", "de-u-ca-gregory", "de-u-hc-h12"]);
    let at = |locale: &str| {
        formatting(
            Stdlib::FormatTime,
            &[
                Value::Number(1790000000000.0),
                Value::Number(0.0),
                Value::str("short"),
                Value::str(locale),
            ],
            &plan,
        )
    };
    assert_eq!(at("de"), Some(Value::str("14:13")));
    assert_eq!(at("de h12"), Some(Value::str("02:13 PM")));
    assert_eq!(at("de h11"), Some(Value::str("14:13")));
    assert_eq!(at("de-u-ca-gregory"), Some(Value::str("14:13")));
    assert_eq!(at("de-u-ca-gregory h12"), Some(Value::str("02:13 PM")));
    assert_eq!(at("de-u-hc-h12"), Some(Value::str("02:13 PM")));
    // The format slot's spelling: the name's own cycle stands over the viewer's.
    let tag = exact_plan::strings::format_tag;
    assert_eq!(
        tag("de-u-ca-gregory", "en-US-u-hc-h12"),
        "de-u-ca-gregory h12"
    );
    assert_eq!(tag("de-u-hc-h12", "en-US-u-hc-h23"), "de-u-hc-h12");
    assert_eq!(tag("de", "en-US-u-rg-gbzzzz-hc-h23"), "de h23");
    assert_eq!(tag("de", "de-DE-u-hc-h11"), "de");
    // A table the plan lacks is refused, never guessed.
    assert_eq!(at("fr"), None);
}
