//! The words of a `transition` or `animation` value, read one way for every
//! reader: this crate's two grammars, the module-scope pass that finds an
//! animation's keyframes name (contract-syntax's `scope.rs`, through
//! [`crate::animation::Slots`]), and through them the kernel, lowering and
//! both web targets. The web target's script readers (`host/web-js`'s
//! `transition-paint.js`, `transition-spring.js`, `shared.js`) fold the same
//! ASCII case, and its tests hold them to this module.
//!
//! @ref LLP 1002 §2, LLP 1055 D5
//!
//! CSS matches a keyword, a unit and a function's name in any ASCII case
//! (CSS Values 4 §2.1, §6.1; CSS Syntax 3 §4.3), and a `<custom-ident>` as
//! written (CSS Values 4 §4.2): `0.3S`, `EASE` and `Cubic-Bezier(…)` are
//! `0.3s`, `ease` and `cubic-bezier(…)`, and a keyframes name keeps its case.
//! A number is CSS's (CSS Syntax 3 §4.3.12), not Rust's, which also reads
//! `inf`, `nan` and `1.`. A refusal names the word as written.

use crate::parse::EASING_FUNCTIONS;

/// Whether `word` is in CSS's number syntax,
/// `[+-]?(\d+|\d*\.\d+)(e[+-]?\d+)?`, with `e` in either case.
pub fn is_number(word: &str) -> bool {
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let unsigned = word.strip_prefix(['+', '-']).unwrap_or(word);
    let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
    let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    let number = match mantissa.split_once('.') {
        Some((int, frac)) => (int.is_empty() || digits(int)) && digits(frac),
        None => digits(mantissa),
    };
    number && digits(exponent)
}

/// The number `word` is, in CSS's syntax ([`is_number`]); one too large for
/// a double is infinite, which the grammars' validation refuses by name.
pub fn number(word: &str) -> Option<f64> {
    is_number(word)
        .then(|| exact_num::parse_f64(word).ok())
        .flatten()
}

/// `word` without its unit `unit` (lower case), in any ASCII case.
pub fn strip_unit<'a>(word: &'a str, unit: &str) -> Option<&'a str> {
    let at = word.len().checked_sub(unit.len())?;
    (word.is_char_boundary(at) && word[at..].eq_ignore_ascii_case(unit)).then(|| &word[..at])
}

/// The seconds a time is: a CSS number, then `s` or `ms` in any case.
pub fn time(word: &str) -> Option<f64> {
    if let Some(n) = strip_unit(word, "ms") {
        return number(n).map(|n| n * 0.001);
    }
    number(strip_unit(word, "s")?)
}

/// Whether `word` is written as a time, a number then `s` or `ms` in any
/// case, even one [`time`] refuses (`1.2.3s`): an identifier that ends in
/// `s` is not.
pub fn time_like(word: &str) -> bool {
    let stem = strip_unit(word, "ms")
        .or_else(|| strip_unit(word, "s"))
        .unwrap_or_default();
    let unsigned = stem.strip_prefix(['+', '-']).unwrap_or(stem);
    let unsigned = unsigned.strip_prefix('.').unwrap_or(unsigned);
    unsigned.starts_with(|c: char| c.is_ascii_digit())
}

/// The keyword of `keywords` (each in lower case) that `word` is, in any
/// ASCII case.
pub fn keyword<'k>(word: &str, keywords: &[&'k str]) -> Option<&'k str> {
    keywords
        .iter()
        .find(|k| word.eq_ignore_ascii_case(k))
        .copied()
}

/// The easing keywords (CSS Easing 1 §2–§4).
pub const EASING_KEYWORDS: [&str; 7] = [
    "linear",
    "ease",
    "ease-in",
    "ease-out",
    "ease-in-out",
    "step-start",
    "step-end",
];

/// The easing function `word` calls, by its name in any case
/// ([`EASING_FUNCTIONS`]): a word that starts `name(`, whatever follows,
/// so a call with a computed argument (`steps(${n}, end)`) is one too.
pub fn easing_call(word: &str) -> Option<&'static str> {
    let (name, _) = word.split_once('(')?;
    EASING_FUNCTIONS
        .iter()
        .map(|(n, _)| *n)
        .find(|n| name.eq_ignore_ascii_case(n))
}

/// Whether `word` is an easing: one of [`EASING_KEYWORDS`] or a call of an
/// easing function, in any case. Whether its arguments are right is the
/// grammar's to say.
pub fn is_easing(word: &str) -> bool {
    keyword(word, &EASING_KEYWORDS).is_some() || easing_call(word).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_keywords_and_function_names_match_in_any_ascii_case() {
        for (word, seconds) in [
            ("0.3s", 0.3),
            ("0.3S", 0.3),
            ("300ms", 0.3),
            ("300MS", 0.3),
            ("300Ms", 0.3),
            ("+.5E1s", 5.0),
        ] {
            assert_eq!(time(word), Some(seconds), "{word}");
            assert!(time_like(word), "{word}");
        }
        // A CSS number, not Rust's: no `inf`, `nan` or `1.`.
        for word in [
            "infs", "INFS", "nans", "1.s", "1..s", "s", "ms", "0.3", "0x1s", "1 s",
        ] {
            assert_eq!(time(word), None, "{word}");
        }
        assert!(time_like("1.s") && time_like("1.2.3S") && !time_like("INFS"));
        assert_eq!(
            keyword("Ease-In-Out", &EASING_KEYWORDS),
            Some("ease-in-out")
        );
        assert_eq!(
            easing_call("Cubic-Bezier(0, 0, 1, 1)"),
            Some("cubic-bezier")
        );
        assert_eq!(easing_call("STEPS("), Some("steps"));
        assert_eq!(easing_call("-EXACT-SPRING()"), Some("-exact-spring"));
        assert_eq!(easing_call("spring(1)"), None);
        assert!(is_easing("LINEAR") && is_easing("Linear(0, 1)") && !is_easing("Opacity"));
    }
}
