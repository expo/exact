//! LLP 1105 D1: `status-bar-style`, `status-bar-animation` and
//! `system-bars-hidden` lower to props, a literal outside their sets is
//! refused, and a bound one follows state.

use crate::source::{boxed, NoData, Source};
use exact_kernel::{Kernel, PropId};
use exact_runner::{Event, Runner};

#[test]
fn the_status_bar_props_lower_and_follow_state() {
    let src = "component App\n  state compact = false\n  action flip\n    compact = not compact\n  view\n    column testId=\"route\" status-bar-style=\"light-content\" system-bars-hidden=true\n      button press=flip testId=\"header\" status-bar-style=(compact ? \"dark-content\" : \"auto\") status-bar-animation=\"fade\" system-bars-hidden=compact\n        text \"Flip\"\n";
    let plan = contract::bake(contract::compile(src).unwrap(), boxed(NoData)).unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let prop = |r: &Runner<Source>, id: &str, p: PropId| {
        let k = r.kernel();
        k.node_by_key(k.find_by_test_id(id)[0])
            .unwrap()
            .props
            .str(p)
            .map(str::to_string)
    };
    let hidden = |r: &Runner<Source>, id: &str| {
        let k = r.kernel();
        k.node_by_key(k.find_by_test_id(id)[0])
            .unwrap()
            .props
            .bool(PropId::SystemBarsHidden)
    };
    assert_eq!(hidden(&r, "route"), Some(true));
    assert_eq!(
        hidden(&r, "header"),
        Some(false),
        "`false` is a declaration"
    );
    assert_eq!(
        prop(&r, "route", PropId::StatusBarStyle).as_deref(),
        Some("light-content")
    );
    assert_eq!(
        prop(&r, "header", PropId::StatusBarStyle).as_deref(),
        Some("auto")
    );
    assert_eq!(
        prop(&r, "header", PropId::StatusBarAnimation).as_deref(),
        Some("fade")
    );
    let header = {
        let k = r.kernel();
        k.node_by_key(k.find_by_test_id("header")[0]).unwrap().id
    };
    r.dispatch(header, Event::Press).unwrap();
    assert_eq!(
        prop(&r, "header", PropId::StatusBarStyle).as_deref(),
        Some("dark-content")
    );
    assert_eq!(hidden(&r, "header"), Some(true));
}

#[test]
fn a_literal_outside_the_set_is_refused() {
    for (attr, value) in [
        ("status-bar-style", "dark"),
        ("status-bar-style", "light"),
        ("navigation-bar-style", "light"),
        ("status-bar-animation", "slide"),
    ] {
        let src = format!("component App\n  view\n    column {attr}=\"{value}\"\n");
        let error = contract::compile(&src).unwrap_err().to_string();
        assert!(
            error.contains("lower-attr-value") || error.contains(attr),
            "{attr}={value}: {error}"
        );
    }
    // A bool's refusal is its type's, not a word list's.
    for value in ["hidden", "yes", "true"] {
        let src = format!("component App\n  view\n    column system-bars-hidden=\"{value}\"\n");
        let error = contract::compile(&src).unwrap_err();
        assert_eq!(error.id, "lower-attr-type", "{value}: {error}");
        assert!(
            error.message.contains("`system-bars-hidden` takes a bool"),
            "{value}: {error}"
        );
    }
    for value in ["light-content", "dark-content", "auto"] {
        contract::compile(&format!(
            "component App\n  view\n    column status-bar-style=\"{value}\"\n"
        ))
        .unwrap();
    }
}
