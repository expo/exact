//! An owner's segments by name (LLP 1123 D16, D10): `--exact-viewport-segment-<field>-<x>-<y>`.

/// The refusal's id and message for a view of `body` under `head` (styles, state), or none
/// where it compiles.
fn refused(head: &str, body: &str) -> Option<(String, String)> {
    let body: String = body.lines().map(|l| format!("      {l}\n")).collect();
    let src = format!("{head}component App\n  view\n    column\n{body}");
    contract::compile(&src).err().map(|e| (e.id, e.message))
}

#[test]
fn an_owner_segment_reads_by_field_and_indices_in_a_length_a_fallback_or_a_class() {
    for field in ["width", "height", "top", "left", "bottom", "right"] {
        let body =
            format!("column padding-top=\"var(--exact-viewport-segment-{field}-1-15, 4px)\"");
        assert_eq!(refused("", &body), None, "{field}");
    }
    // A read with no fallback, which only the host's names may make, and one in a class style
    let bare = "column width=\"var(--exact-viewport-segment-width-0-0)\"";
    assert_eq!(refused("", bare), None);
    let style = "style Pad\n  padding-top=\"var(--exact-viewport-segment-left-1-0, 4px)\"\n\n";
    assert_eq!(refused(style, "column class=Pad"), None);
}

#[test]
fn an_owner_segment_with_another_index_or_field_is_refused_and_one_is_never_declared() {
    // Each index from 0 to 15 as written, and six fields
    for name in [
        "width-16-0",
        "width-01-0",
        "middle-0-0",
        "width-0",
        "width-0-0-0",
    ] {
        let body = format!("column padding-top=\"var(--exact-viewport-segment-{name}, 4px)\"");
        let (id, _) = refused("", &body).expect(name);
        assert_eq!(id, "lower-attr-value", "{name}");
    }
    let declared = "column --exact-viewport-segment-width-0-0=\"4px\"";
    let (id, message) = refused("", declared).expect("declared");
    assert_eq!(id, "lower-host-name-declared");
    assert!(
        message.contains("`var(--exact-viewport-segment-width-0-0)`"),
        "{message}"
    );
}

#[test]
fn a_template_names_only_the_hosts_names_and_an_inset_override_reads_no_segment() {
    // A template's text is checked as a literal is: an index past 15 names no host's value
    let state = "component App\n  state n = 4\n  view\n    column\n";
    let template =
        "column padding-top=`calc(var(--exact-viewport-segment-width-16-0, 4px) + ${n}px)`";
    let src = format!("{state}      {template}\n");
    let e = contract::compile(&src).expect_err("a template");
    assert_eq!(e.id, "lower-attr-value");
    assert!(
        e.message.contains("no name a host defines"),
        "{}",
        e.message
    );
    // An inset override reads none, since no report of the owner's segments reaches it
    let body = "column --exact-safe-area-inset-top=\"var(--exact-viewport-segment-left-1-0, 4px)\"";
    let (id, message) = refused("", body).expect("an override");
    assert_eq!(id, "lower-attr-value");
    assert!(message.contains("reads no owner segment"), "{message}");
}
