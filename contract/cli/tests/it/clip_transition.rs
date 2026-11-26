//! @ref LLP 1003.003 D1, D4 — `transition` names a box's `clip-path`;
//! lowering refuses it on SVG elements, whose scene draws the authored clip,
//! and in `@keyframes`, which do not take it yet.

fn compile(view: &str) -> Result<(), String> {
    contract::compile(&format!(
        "component App\n  state open = true\n  view\n{view}"
    ))
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[test]
fn a_box_takes_a_clip_path_transition_and_an_svg_element_does_not() {
    for transition in [
        "clip-path 300ms ease",
        "all 300ms ease",
        "opacity 1s, clip-path 2s linear",
    ] {
        compile(&format!(
            "    box clip-path=(open ? \"path('M0 0H20V20Z')\" : \"path('M0 0H10V20Z')\") transition=\"{transition}\"\n"
        ))
        .unwrap_or_else(|e| panic!("{transition}: {e}"));
    }
    for view in [
        "    svg transition=\"clip-path 300ms\"\n",
        "    svg\n      rect width=20 height=20 transition=\"clip-path 300ms\"\n",
    ] {
        let message = compile(view).expect_err(view);
        assert!(
            message.contains("names `clip-path`") && message.contains("surrounding box"),
            "{message}"
        );
    }
    compile("    svg\n      rect width=20 height=20 transition=\"all 300ms\"\n")
        .expect("`all` covers what an SVG element moves");
}

#[test]
fn a_keyframe_cannot_name_clip_path() {
    let source = "keyframes shrink\n  to clip-path=\"path('M0 0H10V20Z')\"\ncomponent App\n  view\n    box clip-path=\"path('M0 0H20V20Z')\" animation=\"shrink 1s\"\n";
    let message = contract::compile(source).expect_err("refused").to_string();
    assert!(message.contains("clip-path"), "{message}");
}
