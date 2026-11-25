//! The events the JS target binds.
/// @ref LLP 1057.001 §8.1 — the web has no swipe of the platform's: a `swipe`
/// is a plain listener of an event nothing fires, and links no motion.
#[test]
fn a_swipe_binds_an_event_nothing_fires() {
    let source = "component App\n  action hide\n    let n = 1\n  view\n    column swipe=hide swipe-flick=\"down\"\n      text \"bar\"\n";
    let emitted = crate::emit::emit(&contract::compile(source).unwrap(), false, false).unwrap();
    assert!(emitted.js.contains("\"swipe\""), "{}", emitted.js);
    assert!(!emitted.motion);
}
