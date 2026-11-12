//! The search tab's words (LLP 1141.002 D1): `rel="search"` on a root tab.

use crate::warnings::{tabs, warned};

/// LLP 1141.002 D1: a root tab that says `rel="search"` is the app's search
/// tab. Written last it compiles quietly; anywhere else, or a second one,
/// is named. `rel` off a tab, a link or a form, and `role="search"` on a
/// tab, are refused with their repair.
#[test]
fn a_search_tab_is_written_last_and_once() {
    const HOME: &str = "  button role=\"tab\" aria-controls=\"panel-home\" aria-selected=true";
    const MORE: &str = "  button role=\"tab\" aria-controls=\"panel-more\" aria-selected=false";
    let last = tabs(&format!(
        "{HOME}\n    text \"Home\"\n{MORE} rel=\"search\"\n    text \"Search\""
    ));
    assert_eq!(warned(&last), []);
    let first = tabs(&format!(
        "{HOME} rel=\"Search\"\n    text \"Home\"\n{MORE}\n    text \"More\""
    ));
    assert_eq!(warned(&first), [("lower-search-tab-last".to_string(), 13)]);
    let both = tabs(&format!(
        "{HOME} rel=\"search\"\n    text \"Home\"\n{MORE} rel=\"search\"\n    text \"More\""
    ));
    assert_eq!(
        warned(&both),
        [
            ("lower-search-tab-second".to_string(), 15),
            ("lower-search-tab-last".to_string(), 13)
        ]
    );
    // Other link types say nothing about search.
    let other = tabs(&format!(
        "{HOME} rel=\"help\"\n    text \"Home\"\n{MORE}\n    text \"More\""
    ));
    assert_eq!(warned(&other), []);
    let off_tab = tabs(&format!(
        "{HOME}\n    text \"Home\" rel=\"search\"\n{MORE}\n    text \"More\""
    ));
    let e = contract::compile(&off_tab)
        .expect_err("rel on a text")
        .to_string();
    assert!(
        e.contains("lower-attr-tag") && e.contains("`rel` belongs to `link` and `form`"),
        "{e}"
    );
    let role = tabs(&format!("  button role=\"search\" aria-controls=\"panel-home\"\n    text \"Home\"\n{MORE}\n    text \"More\""));
    let e = contract::compile(&role)
        .expect_err("role=search on a tab")
        .to_string();
    assert!(
        e.contains("keep `role=\"tab\"`; write `rel=\"search\"`"),
        "{e}"
    );
}
