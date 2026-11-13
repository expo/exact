//! The document's language on the Apple host (LLP 1060 D4): the resolved
//! table's `lang` and `dir`, published at boot and when the place moves.

use crate::host::NoData;
use exact_apple::Host;
use exact_kernel::MonospaceMeasurer;

#[test]
fn resolved_language_is_published_at_boot_and_after_switching() {
    let mut plan = contract::compile("component App\n  state language = \"en\"\n  view\n    column width=300\n      text \"123\" testId=\"text\"\n").unwrap();
    plan.locale = Some(exact_plan::SlotsId(0));
    // No format data: the formatters print en-US (LLP 1054.000.003 D6).
    let format = exact_plan::StrId(plan.strings.len() as u32);
    plan.strings.push("");
    for rtl in [false, true] {
        let name = exact_plan::StrId(plan.strings.len() as u32);
        plan.strings.push(if rtl { "ar" } else { "en" });
        plan.locales.push(exact_plan::LocalesRow {
            name,
            rtl,
            format,
            texts: Default::default(),
        });
    }
    let plan = contract::bake(plan, NoData).unwrap();
    let (mut host, batch) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.0,
        300.0,
    )
    .unwrap();
    assert!(batch.contains(r#"{"op":"language","lang":"en","dir":"ltr"}"#));
    let batch = host.set_place("ar-EG", "UTC", None, None);
    assert!(
        batch.contains(r#"{"op":"language","lang":"ar","dir":"rtl"}"#),
        "{batch}"
    );
    assert!(batch.contains(r#""direction":"rtl""#), "{batch}");
    assert!(!host
        .set_place("ar-SA", "UTC", None, None)
        .contains(r#""op":"language""#));
    let batch = host.set_place("de", "UTC", None, None);
    assert!(batch.contains(r#"{"op":"language","lang":"en","dir":"ltr"}"#));
}
