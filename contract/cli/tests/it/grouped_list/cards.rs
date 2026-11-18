//! A section's card: the system draws it, and only the literal `"transparent"` drops it.

use super::*;

#[test]
fn a_transparent_section_has_no_card() {
    let r = boot("list appearance=\"auto\" testId=\"list\" flex=1\n  section background-color=\"transparent\" testId=\"head\"\n    row testId=\"profile\"\n      image \"avatar.png\" width=80 height=80\n      text \"Maya Chen\"\n  section\n    button press=go testId=\"mute\"\n      text \"Mute\"");
    let k = r.kernel();
    let list = k.grouped_list(id(&r, "list")).unwrap();
    assert!(
        !list.sections[0].card,
        "background-color=\"transparent\" drops the card"
    );
    assert!(list.sections[1].card, "a section is on a card by default");
    // The sheet's group (the rows' column) takes the colour; the section
    // itself keeps none, so the web and the sheet's hosts draw no card either.
    let head = k.node(id(&r, "head")).unwrap();
    let group = k.node(head.children()[0]).unwrap();
    let transparent = group
        .style
        .background_color
        .is_some_and(|c| c.resolve(false).a() == 0 && c.resolve(true).a() == 0);
    assert!(transparent, "the group's background is the section's");
    assert!(
        head.style
            .background_color
            .is_none_or(|c| c.resolve(false).a() == 0),
        "and the section paints none of its own"
    );
}

#[test]
fn a_cardless_section_draws_no_separators_and_takes_only_transparent() {
    let r = boot("list appearance=\"auto\" testId=\"list\" flex=1\n  section background-color=\"transparent\"\n    button press=go testId=\"a\"\n      text \"A\"\n    button press=go testId=\"b\"\n      text \"B\"");
    let k = r.kernel();
    assert_eq!(
        k.grouped_separator(id(&r, "a")),
        None,
        "no separator under a card-less row"
    );
    // A colour, and a computed value even when it comes to "transparent", are refused, each saying why.
    for (value, why) in [
        ("\"#ff0000\"", "a colour is refused"),
        (
            "(1 > 0 ? \"transparent\" : \"#ff0000\")",
            "two `section`s under an `if`",
        ),
    ] {
        let m = refused(&format!(
            "list appearance=\"auto\" flex=1\n  section background-color={value}\n    text \"A\""
        ));
        assert!(
            m.contains("may only be the literal `\"transparent\"`") && m.contains(why),
            "{m}"
        );
    }
}
