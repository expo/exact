use super::*;

#[test]
fn declarations_are_authors_names_and_lengths() {
    let p = SafeAreaProperties::parse(" --gap : 8px ; --pad: calc(var(--gap) * 2);--gap: 12px; ")
        .unwrap();
    assert_eq!(p.css(), "--pad: calc(2 * var(--gap)); --gap: 12px");
    assert_eq!(SafeAreaProperties::parse(&p.css()), Some(p));
    // Every length a calculation tree holds, as written; a name escaped where it must be
    let each = SafeAreaProperties::parse(
        "--a: 2rem; --b: 50%; --c: 10vw; --d: env(safe-area-inset-top); --e: 1in; --f\\?: var(--exact-safe-area-inset-top); --g: var(--a, calc(10% + 2px))",
    )
    .unwrap();
    let css = "--a: 2rem; --b: 50%; --c: 10vw; --d: env(safe-area-inset-top); --e: 96px; --f\\?: var(--exact-safe-area-inset-top); --g: var(--a, calc(10% + 2px))";
    assert_eq!(each.css(), css);
    assert_eq!(SafeAreaProperties::parse(css), Some(each));
    // No declaration is a node that declares none
    assert_eq!(
        SafeAreaProperties::parse(" "),
        Some(SafeAreaProperties::default())
    );
    for bad in [
        "gap: 8px",
        "--exact-safe-area-inset-top: 0px",
        "--a: 1px !important",
        "--a: red",
        "--a: auto",
        "--a: var(--b, red)",
        "--a 1px",
        "--a: 1px --b: 2px",
        // A font-relative length converts where its row is set, so a declaration takes `rem`
        "--a: 1em",
        // An `env()` with a fallback is a text a row substitutes, so no tree holds it
        "--a: env(safe-area-inset-top, 4px)",
        "--a: red env(safe-area-inset-top, 0px)",
        // Outside a math function a fallback stands alone
        "--a: var(--b, 10% + 2px)",
        // A container unit resolves against a box's query containers, which a value resolved
        // where it is declared does not know yet (LLP 1123 D16)
        "--a: 50cqw",
        "--a: calc(100% - 10cqmin)",
    ] {
        assert_eq!(SafeAreaProperties::parse(bad), None, "{bad:?}");
    }
    assert_eq!(
        SafeAreaProperties::value("1px; --b: 8px"),
        None,
        "one value"
    );
    // A fallback is one value wherever the call stands: CSS substitutes its
    // tokens, so a bare sum would group differently
    assert_eq!(
        SafeAreaProperties::value("calc(2 * var(--m, 1px + 2px))"),
        None
    );
    assert!(SafeAreaProperties::value("calc(2 * var(--m, calc(1px + 2px)))").is_some());
}
