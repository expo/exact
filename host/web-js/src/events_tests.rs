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

/// @ref LLP 1057.001 §4.1 — touch-web pinch stays silent (expo/exact#1804)
/// and emits no listener or motion dependency.
#[test]
fn pinch_and_pinchrelease_emit_no_web_listeners() {
    let source = "component App\n  state s = 1\n  action onPinch(scale: number, x: number, y: number)\n    s = scale\n  view\n    box pinch=onPinch pinchrelease=onPinch\n      text `${s}`\n";
    let emitted = crate::emit::emit(&contract::compile(source).unwrap(), false, false).unwrap();
    assert!(!emitted.js.contains("\"pinch\""), "{}", emitted.js);
    assert!(!emitted.js.contains("\"pinchrelease\""), "{}", emitted.js);
    assert!(!emitted.js.contains("$on("), "{}", emitted.js);
    assert!(!emitted.motion);
}
