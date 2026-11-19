//! A native target's raw safe area by name (LLP 1123 D9, D10), the host's names declared
//! (`lower-host-name-declared`), and a reservation that reads a raw observation
//! (`lower-reservation-reads-raw`, D5's provenance).

/// The refusal's id and message for a view of `body` under `styles`, or none where it
/// compiles.
fn refused(styles: &str, body: &str) -> Option<(String, String)> {
    let body: String = body.lines().map(|l| format!("      {l}\n")).collect();
    let src = format!("{styles}component App\n  view\n    column\n{body}");
    contract::compile(&src).err().map(|e| (e.id, e.message))
}

#[test]
fn a_native_targets_safe_area_reads_by_number_kind_relation_and_name() {
    for name in [
        "native-2",
        "navigation",
        "tab-content",
        "window",
        "target-map",
    ] {
        let body = format!("column padding-top=\"var(--exact-{name}-safe-area-inset-top, 4px)\"");
        assert_eq!(refused("", &body), None, "{name}");
    }
    // No chain entry 0 or past 255, a number as written, a registered name by the registry's
    // rule, and a target's only family is the safe area
    for name in [
        "native-0",
        "native-256",
        "native-02",
        "target-Map",
        "native-2-layout-margins",
    ] {
        let body = format!("column padding-top=\"var(--exact-{name}-safe-area-inset-top, 4px)\"");
        let (id, _) = refused("", &body).expect(name);
        assert_eq!(id, "lower-attr-value", "{name}");
    }
}

#[test]
fn a_host_name_is_read_never_declared() {
    // On an element and in a class style; a name no host defines is said to be none
    let style = |name: &str| format!("style Pad\n  {name}=\"4px\"\n\n");
    for name in [
        "--exact-native-2-safe-area-inset-top",
        "--exact-keyboard-inset-bottom",
    ] {
        let on_element = refused("", &format!("column {name}=\"4px\""));
        let in_style = refused(&style(name), "column class=Pad");
        for (id, message) in [on_element.expect(name), in_style.expect(name)] {
            assert_eq!(id, "lower-host-name-declared");
            assert!(message.contains(&format!("`var({name})`")), "{message}");
        }
    }
    let (id, message) = refused("", "column --exact-foo=\"4px\"").expect("--exact-foo");
    assert_eq!(id, "lower-host-name-declared");
    assert!(
        message.starts_with("`--exact-foo` is no name a host defines"),
        "{message}"
    );
}

#[test]
fn a_reservation_reads_only_incoming_observations() {
    // A raw observation directly, in a fallback, and through a capture its parent declares
    let raw = [
        ("column -exact-safe-area-padding=\"var(--exact-native-1-safe-area-inset-top)\"", "--exact-native-1-safe-area-inset-top"),
        ("column -exact-additional-safe-area=\"var(--gap, var(--exact-window-safe-area-inset-top))\"", "--exact-window-safe-area-inset-top"),
        ("column --x=\"var(--exact-system-minimum-layout-margins-inset-left)\"\n  column -exact-safe-area-padding=\"0 var(--x)\"", "--x"),
    ];
    for (body, read) in raw {
        let (id, message) = refused("", body).expect(body);
        assert_eq!(id, "lower-reservation-reads-raw", "{body}");
        assert!(
            message.contains(&format!("reads `var({read})`")),
            "{message}"
        );
    }
    // A capture and a reservation in class styles, and a capture that reads one declared
    // after it
    let styled = "style Raw\n  --x=\"var(--exact-native-1-safe-area-inset-top)\"\n\nstyle Pad\n  -exact-safe-area-padding=\"var(--x)\"\n\n";
    let (id, _) = refused(styled, "column class=Raw\n  column class=Pad").expect("styled");
    assert_eq!(id, "lower-reservation-reads-raw");
    let later = "column --y=\"var(--x)\" --x=\"var(--exact-window-safe-area-inset-top)\"\n  column -exact-safe-area-padding=\"var(--y)\"";
    let (id, _) = refused("", later).expect("later");
    assert_eq!(id, "lower-reservation-reads-raw");
    // A property is checked by its name: one declaration that reads a raw observation makes
    // every read of the name in a reservation refused
    let named = "column --inset=\"var(--exact-native-1-safe-area-inset-top)\"\n  column padding-top=\"var(--inset)\"\ncolumn --inset=\"var(--exact-keyboard-inset-bottom)\"\n  column -exact-safe-area-padding=\"0 0 var(--inset) 0\"";
    let (_, message) = refused("", named).expect("named");
    assert!(
        message.contains("a declaration of `--inset` on line"),
        "{message}"
    );
    // The box's own keyboard reading is incoming, and a raw one may pad an ordinary box
    let incoming = "column -exact-safe-area-padding=\"0 0 var(--exact-keyboard-inset-bottom) 0\"\ncolumn padding-top=\"var(--exact-native-1-safe-area-inset-top, 0px)\"";
    assert_eq!(refused("", incoming), None);
}
