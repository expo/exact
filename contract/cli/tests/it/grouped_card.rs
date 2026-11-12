// A grouped list's section card as its author styles it.
use super::grouped_list::{boot, id, refused};

#[test]
fn a_sections_shadow_is_its_cards() {
    let r = boot("list appearance=\"auto\" testId=\"list\" flex=1\n  section box-shadow=\"0 4px 14px rgb(29 22 30 / 0.05)\" testId=\"s\"\n    button press=go\n      text \"A\"");
    let k = r.kernel();
    let list = k.grouped_list(id(&r, "list")).unwrap();
    let section = k.node(id(&r, "s")).unwrap();
    assert_eq!(
        list.sections[0].card_view,
        section.children()[0],
        "the card is the section's group, which holds its rows"
    );
    let card = k.node(list.sections[0].card_view).unwrap();
    assert_eq!(
        card.style.box_shadow.shadows().len(),
        1,
        "the card casts the shadow"
    );
    assert!(
        section.style.box_shadow.shadows().is_empty(),
        "and the section none of its own, so the web casts it around the card"
    );
    let m = refused("list appearance=\"auto\" flex=1\n  section background-color=\"transparent\" box-shadow=\"0 1px 2px black\"\n    text \"A\"");
    assert!(m.contains("box-shadow"), "{m}");
}
