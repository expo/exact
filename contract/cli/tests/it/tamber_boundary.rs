//! A mode boundary's implied rows (LLP 1121 D7): the inherited colour text
//! set again in the boundary's own tuple.
use super::tamber::{app, booted, parsed, press, row};
use exact_kernel::StyleId;

/// Inherited text the boundary's tuple refuses is not kept as set: the row
/// it implied before goes, and the next visit parses the text again.
#[test]
fn a_refused_implied_colour_is_parsed_again_on_the_next_visit() {
    let path = app(
        "boundary-refused",
        &[
            ("app.tamber", "tamber t\n\naxes\n  dark-style = black | dim  under scheme=dark\n\ntheme\n  ink = light-dark(#111111, #eeeeee)\n    when dark-style=dim: light-dark(#111111, #cccccc)\n  soft = light-dark(#555555, #999999)\n    when dark-style=dim: light-dark(#555555, #777777)\n"),
            ("app.contract", "component App\n  state quiet = false\n  state n = 0\n  action flip\n    quiet = not quiet\n  action bump\n    n = n + 1\n  view\n    column color=(quiet ? \"soft\" : \"ink\") testId=\"outer\"\n      button press=flip testId=\"flip\"\n        text \"flip\"\n      button press=bump testId=\"bump\"\n        text \"bump\"\n      column -exact-mode=\"dark-style=dim\" opacity=(n > 0 ? 0.5 : 1)\n        text \"b\" testId=\"inner\"\n"),
        ],
    );
    let mut r = booted(contract::compile_path(&path).unwrap());
    let soft = parsed("light-dark(#555555, #777777)");
    press(&mut r, "flip");
    assert_eq!(row(&r, "inner", StyleId::TextColor), soft);
    // `ink` parses in the outer tuple, `black`, and not in the boundary's
    // `dim` one (an edit gives each tuple's text, in tuple order).
    let black = "light-dark(#111111, #eeeeee)";
    r.set_token("ink", black, &[black, "nope"]).unwrap();
    press(&mut r, "flip");
    assert_eq!(row(&r, "outer", StyleId::TextColor), parsed(black));
    assert_eq!(
        row(&r, "inner", StyleId::TextColor),
        parsed(black),
        "the refused text implies nothing: the colour around it reaches the text"
    );
    let dim = "light-dark(#111111, #aaaaaa)";
    r.set_token("ink", black, &[black, dim]).unwrap();
    press(&mut r, "bump");
    assert_eq!(row(&r, "inner", StyleId::TextColor), parsed(dim));
}
