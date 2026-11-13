//! A `match` among a grouped row's texts is read as an `if` there is: the
//! standard cell counts its texts (`grouped/cell.rs`), and in a stack of
//! lines its text takes the subtitle's line (`grouped/lines.rs`).

use exact_kernel::{Kernel, Offer, StyleMask};
use exact_runner::{DataError, DataSource, Runner, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// `body` under a 402 × 874 column, with a `dark` flag and an optional
/// `lock` that holds a string.
fn boot(body: &str) -> Runner<NoData> {
    let body = body
        .lines()
        .map(|l| format!("      {l}\n"))
        .collect::<String>();
    let source = format!(
        "component App\n  state dark = false\n  state on = true\n  state lock: option<string> = some(\"Screen lock\")\n  action go\n  action flip(v: bool)\n    on = v\n  view\n    column testId=\"root\" width=402 height=874\n{body}"
    );
    let plan = contract::bake(
        contract::compile(&source).unwrap_or_else(|e| panic!("{e}\n{source}")),
        NoData,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel_mut();
    let root = k.node_by_key(k.find_by_test_id("root")[0]).unwrap().id;
    k.compute_layout(root, Offer::definite(402.0, 874.0))
        .unwrap();
    r
}

fn id(r: &Runner<NoData>, test_id: &str) -> u32 {
    let k = r.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

#[test]
fn a_match_among_a_rows_texts_is_read_as_an_if() {
    let by_if =
        "if dark\n  text \"Screen lock\" testId=\"SLOT\"\nelse\n  text \"Off\" testId=\"SLOT\"";
    let by_match = "match lock\n  case some(l)\n    text l testId=\"SLOT\"\n  case none\n    text \"Off\" testId=\"SLOT\"";
    let indent = |choice: &str, slot: &str| -> String {
        let choice = choice.replace("SLOT", slot);
        choice.lines().map(|l| format!("        {l}\n")).collect()
    };
    for choice in [by_if, by_match] {
        let r = boot(&format!(
            "list appearance=\"auto\" testId=\"list\" flex=1\n  section\n    button press=go\n      column\n        text \"Privacy\" testId=\"cell-title\"\n{}    row\n      input type=\"checkbox\" checked=on input=flip\n      column flex=1 min-width=0\n        text \"Milk\"\n{}",
            indent(choice, "cell-detail"),
            indent(choice, "line"),
        ));
        let k = r.kernel();
        let list = k.grouped_list(id(&r, "list")).unwrap();
        assert!(
            list.sections[0].rows[0].subtitle,
            "the subtitle cell:\n{choice}"
        );
        let size = |t: &str| {
            k.node(id(&r, t))
                .unwrap()
                .computed_style(StyleMask::ALL)
                .font_size
        };
        assert_eq!(
            (size("cell-title"), size("line")),
            (16.0, 14.0),
            "the stack's second line is the subheadline:\n{choice}"
        );
    }
}
