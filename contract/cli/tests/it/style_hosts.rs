//! @ref LLP 1159.000 D4 — `style-undrawn`: for each target the app ships,
//! each style row it sets that the target's host does not draw there (the
//! schema's `hosts` column), once a row, scope and reason, naming each
//! target; `best-effort` says nothing; a value that leaves the row unset
//! is not judged.

use crate::scratch::Scratch;

fn app(name: &str, targets: &str, view: &str) -> Scratch {
    let dir = Scratch::new(format!("exact2-style-hosts-{name}-{}", std::process::id()));
    std::fs::write(
        dir.join("app.json"),
        format!("{{\"app\": {{\"id\": \"dev.test.styles\", \"name\": \"Styles\"}}, \"host\": {{{targets}}}}}"),
    )
    .unwrap();
    std::fs::write(dir.join("app.contract"), view).unwrap();
    dir.at("app.contract")
}

/// The `style-undrawn` messages, in source order.
fn undrawn(path: &std::path::Path) -> Vec<String> {
    let (plan, warnings) = contract::compile_path_all_warned(path, false);
    plan.unwrap_or_else(|e| panic!("{e:?}"));
    warnings
        .iter()
        .filter(|w| w.id == "style-undrawn")
        .map(|w| w.message.clone())
        .collect()
}

/// Two blended shapes, one by its own row and one by a class, and a
/// filtered box.
const BLENDED: &str = "style Multiplied\n  mix-blend-mode=\"multiply\"\n\ncomponent App\n  view\n    column\n      box filter=\"blur(4px)\" width=20 height=20\n      svg width=20 height=20\n        rect width=10 height=10 mix-blend-mode=\"screen\"\n        rect width=10 height=10 class=Multiplied\n";

#[test]
fn svg_blends_draw_on_every_graphical_host() {
    assert_eq!(undrawn(&app("blend", "\"ios\": {}, \"tvos\": {}, \"macos\": {}, \"web\": {}, \"android\": {}, \"linux\": {}, \"windows\": {}", BLENDED)), Vec::<String>::new());
}

#[test]
fn best_effort_and_an_unset_value_say_nothing() {
    // A backdrop blur differs on iOS and macOS, and a turned box's shadow
    // where the paint walk draws: `vocab` says both, the lint neither.
    let glass = "component App\n  view\n    column\n      box backdrop-filter=\"blur(8px)\" filter=\"drop-shadow(2px 2px 2px #000000)\" width=20 height=20\n      svg width=20 height=20\n        rect width=10 height=10 mix-blend-mode=\"normal\"\n";
    assert_eq!(
        undrawn(&app(
            "glass",
            "\"ios\": {}, \"macos\": {}, \"linux\": {}, \"android\": {}",
            glass
        )),
        Vec::<String>::new()
    );
}

#[test]
fn it_is_information_like_forma_unreachable() {
    assert!(contract_lower::warning_rank::information("style-undrawn"));
}
