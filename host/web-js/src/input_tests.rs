//! LLP 1047.002 D4.2 — the input piece's behaviours a page leaves out: each one
//! the plan cannot make what it acts on, a bound value counting as every word
//! it could take.

use crate::emit::INPUT as ALL;

const NONE: [&str; 0] = [];

/// The behaviours a page holding `body` uses: a column that presses, then `body`.
fn used(body: &str) -> Vec<&'static str> {
    let source = format!("component App\n  state on = true\n  action go\n    on = !on\n  action drag(dx: number, dy: number)\n    on = dx > dy\n  view\n    column press=go\n{body}");
    let left = crate::emit::emit(&contract::compile(&source).unwrap(), false, false)
        .unwrap()
        .input;
    ALL.into_iter().filter(|b| !left.contains(b)).collect()
}

#[test]
fn a_page_that_only_presses_leaves_every_behaviour_out() {
    assert_eq!(used("      text \"Go\"\n"), NONE);
}

#[test]
fn a_press_paint_and_a_state_look_link_theirs() {
    assert_eq!(
        used("      :active\n        opacity = 0.6\n      text \"Go\"\n"),
        ["PRESS_PAINT"]
    );
    assert_eq!(
        used("      :active\n        background-color = \"red\"\n      text \"Go\"\n"),
        ["PRESS_PAINT"]
    );
    assert_eq!(
        used("      :hover\n        border-color = \"red\"\n      text \"Go\"\n"),
        ["STATE_LOOKS"]
    );
    // A press scale is the piece's always.
    assert_eq!(
        used("      :active\n        -exact-press-scale = 0.95\n      text \"Go\"\n"),
        NONE
    );
}

#[test]
fn a_pan_a_role_and_a_shortcut_link_theirs() {
    assert_eq!(
        used("      column pan=drag\n        text \"Drag\"\n"),
        ["PANS"]
    );
    for role in ["tablist", "menu", "alertdialog"] {
        assert_eq!(
            used(&format!("      row role=\"{role}\"\n")),
            ["ROVING"],
            "{role}"
        );
    }
    assert_eq!(used("      row role=\"radiogroup\"\n"), ["RADIO_GROUPS"]);
    // A bound role may be any.
    assert_eq!(
        used("      row role=(on ? \"radiogroup\" : \"group\")\n"),
        ["ROVING", "RADIO_GROUPS"]
    );
    assert_eq!(
        used("      button \"Find\" press=go aria-keyshortcuts=\"Control+K\"\n"),
        ["SHORTCUTS"]
    );
}
