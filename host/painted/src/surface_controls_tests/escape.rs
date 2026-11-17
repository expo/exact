//! Escape from a field leaves the focus where it is, as a browser does,
//! unless a canvas in the app takes keys: then it hands them back to it.

use super::{boot, find};

#[test]
fn escape_keeps_the_focus_in_an_app_without_a_keyed_canvas() {
    let (mut p, _) = boot(
        "component App\n  state note = \"\"\n  action typed(value: string)\n    note = value\n  view\n    column width=100 height=300\n      input testId=\"field\" value=note input=typed height=30\n      button \"Save\" testId=\"save\" height=40\n",
        &[],
    );
    for name in ["field", "save"] {
        let target = find(&p, name);
        p.focus = Some(target);
        p.hardware_key("Escape", "Escape", true, false);
        p.hardware_key("Escape", "Escape", false, false);
        assert_eq!(p.focus(), Some(target), "{name} kept the focus");
    }
}
