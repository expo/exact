//! An observation's `start` and `end` by name (LLP 1123 D9), which resolve against the reading
//! box's `direction`.

fn compiled(body: &str) -> Result<(), (String, String)> {
    let src = format!("component App\n  view\n    column\n      {body}\n");
    contract::compile(&src)
        .map(|_| ())
        .map_err(|e| (e.id, e.message))
}

#[test]
fn a_family_and_a_target_read_their_start_and_end_and_the_safe_area_by_edge_alone() {
    for name in [
        "keyboard-inset-start",
        "layout-margins-inset-end",
        "native-2-safe-area-inset-start",
        "window-safe-area-inset-end",
    ] {
        let body = format!("column padding-top=\"var(--exact-{name}, 4px)\"");
        assert_eq!(compiled(&body), Ok(()), "{name}");
    }
    // The box's own safe area is the local inset, by edge, until activation
    for name in ["keyboard-inset-middle", "safe-area-inset-start"] {
        let body = format!("column padding-top=\"var(--exact-{name}, 4px)\"");
        let (id, _) = compiled(&body).expect_err(name);
        assert_eq!(id, "lower-attr-value", "{name}");
    }
}
