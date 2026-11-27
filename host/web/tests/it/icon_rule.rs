//! @ref LLP 1121 stage 14 C2 — an icon's fill rule on the web host: the
//! element the mask is painted for (glue.js `refreshSymbols` writes each
//! `<path>`'s `fill-rule` from it; tests/symbol-hole.test.mjs draws it).

use exact_web::Host;

/// Stage 14 C2: an even-odd icon's form (`evenodd:` before its path) draws
/// its path, the marker stripped, under its viewBox, marked even-odd for
/// the mask (`data-symbol-evenodd`, not `data-symbol-nonzero`); a nonzero
/// icon is as before; a role's even-odd silhouette is marked neither, as
/// before.
#[test]
fn an_even_odd_icon_is_marked_for_its_mask() {
    let plan = contract::compile(
        "component App\n  view\n    column\n      image \"symbol:icon||0 0 24 24|evenodd:M2 2H22V22H2Z M8 8H16V16H8Z|icons/ring.svg\"\n      image \"symbol:icon||0 0 24 24|M2 2H22V22H2Z|icons/sq.svg\"\n      image \"symbol:bookmark-fill\"\n",
    )
    .unwrap();
    let (_, first) = Host::boot(
        &plan.encode(),
        caltrain_data::Caltrain,
        Default::default(),
        "/",
    )
    .unwrap();
    let ops: Vec<&str> = first
        .split("{\"op\":")
        .filter(|o| o.contains("data-symbol-source"))
        .collect();
    let [ring, square, role] = ops[..] else {
        panic!("{first}")
    };
    assert!(
        ring.contains("\"data-symbol-path\":\"M2 2H22V22H2Z M8 8H16V16H8Z\""),
        "{ring}"
    );
    assert!(ring.contains("\"data-symbol-evenodd\":\"\""), "{ring}");
    assert!(
        ring.contains("\"data-symbol-viewbox\":\"0 0 24 24\""),
        "{ring}"
    );
    assert!(!ring.contains("data-symbol-nonzero"), "{ring}");
    assert!(square.contains("\"data-symbol-nonzero\":\"\""), "{square}");
    assert!(!square.contains("data-symbol-evenodd"), "{square}");
    assert!(
        !role.contains("data-symbol-evenodd") && !role.contains("data-symbol-nonzero"),
        "{role}"
    );
}

#[test]
fn directional_roles_emit_data_symbol_mirror_and_fixed_or_icon_sources_do_not() {
    let plan = contract::compile(
        "component App\n  view\n    column\n      image \"symbol:back\"\n      image \"symbol:sf/chevron.forward\"\n      image \"symbol:send\"\n      image \"symbol:play\"\n      image \"symbol:icon|chevron.backward|0 0 24 24|M15 5 8 12l7 7|icons/back.svg\"\n",
    )
    .unwrap();
    let (_, first) = Host::boot(
        &plan.encode(),
        caltrain_data::Caltrain,
        Default::default(),
        "/",
    )
    .unwrap();
    let ops: Vec<&str> = first
        .split("{\"op\":")
        .filter(|o| o.contains("data-symbol-source"))
        .collect();
    let [back, forward, send, play, icon] = ops[..] else {
        panic!("{first}")
    };
    assert!(back.contains("\"data-symbol-mirror\":\"\""), "{back}");
    assert!(forward.contains("\"data-symbol-mirror\":\"\""), "{forward}");
    assert!(
        !send.contains("data-symbol-mirror"),
        "web send is arrow.up: {send}"
    );
    assert!(!play.contains("data-symbol-mirror"), "{play}");
    assert!(!icon.contains("data-symbol-mirror"), "{icon}");
}
