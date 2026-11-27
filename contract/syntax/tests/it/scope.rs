//! Rescoping: a literal's names take the program-unique names they mean.

use contract_syntax::parse;
use contract_syntax::scope::{rescope, Kind, Scope};

/// An `animation` literal's words are split at any CSS white space, as
/// `motion` splits them, so its keyframes name is renamed after a tab or a
/// line break as after a space.
#[test]
fn an_animations_keyframes_name_is_renamed_after_any_white_space() {
    for value in [r"1s spin", r"1s\nspin", r"spin\t1s", r"1s\u{d}\nspin"] {
        let src = format!(
            "keyframes spin\n  to opacity=0\n\ncomponent App\n  view\n    text \"a\" animation=\"{value}\"\n"
        );
        let mut file = parse(&src).unwrap();
        let mut scope = Scope::default();
        scope
            .names
            .insert((Kind::Keyframes, "spin".into()), "spin_lib".into());
        rescope(&mut file, &scope).unwrap();
        // The declaration and the literal.
        let printed = format!("{file:?}");
        assert_eq!(printed.matches("spin_lib").count(), 2, "{value}: {printed}");
    }
}
