//! `translate`'s value as the engine's four components, for the readers
//! below the kernel: a keyframe's `translate`, and the JS target's motion
//! seam, which reads a bound `translate` without the kernel. Each axis is a
//! `<length-percentage>` of the box (CSS Transforms 2 §5.1): a length, a
//! percentage, or a `calc()` of them (CSS Values 4 §10.1), held as px plus a
//! percentage, which is how CSS interpolates it. The kernel reads the
//! `translate` attribute with its own math functions (`exact_kernel`'s
//! `space::translate`), which also take `min()`, `max()` and `clamp()` that
//! simplify to px plus a percentage; this grammar is `calc()`'s sums and
//! products.

use crate::parse::split_top_level;
use crate::{word, Value};

/// `translate` as lengths in px then percentages of the box: x, y, x%, y%.
/// One or two axes; with `z`, a third value, a px length, which the engine
/// does not animate (it is the kernel's `translate_z` row).
pub fn translate(text: &str, z: bool) -> Option<Value> {
    // CSS white space (CSS Syntax 3 §4.2) separates the values.
    let spaced = text.replace(['\t', '\n', '\r', '\u{c}'], " ");
    let parts: Vec<&str> = split_top_level(&spaced, ' ')
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    let (x, y) = match parts[..] {
        [x] => (axis(x)?, (0.0, 0.0)),
        [x, y] => (axis(x)?, axis(y)?),
        [x, y, d] if z && axis(d).is_some_and(|(_, pct)| pct == 0.0) => (axis(x)?, axis(y)?),
        _ => return None,
    };
    Some(Value::four(x.0, y.0, x.1, y.1))
}

/// One axis as px and a percentage. A number without a unit is px, as the
/// keyframe grammar's lengths are.
fn axis(token: &str) -> Option<(f64, f64)> {
    let number = |n: &str| {
        exact_num::parse_f64(n.trim())
            .ok()
            .filter(|n| n.is_finite())
    };
    if let Some(n) = word::strip_unit(token, "%") {
        return number(n).map(|n| (0.0, n));
    }
    if let Some(n) = number(word::strip_unit(token, "px").unwrap_or(token)) {
        return Some((n, 0.0));
    }
    if !token
        .as_bytes()
        .get(..5)
        .is_some_and(|f| f.eq_ignore_ascii_case(b"calc("))
    {
        return None;
    }
    let mut p = Calc {
        s: token.as_bytes(),
        at: 0,
        depth: 0,
    };
    match (p.value()?, p.at == token.len()) {
        (Term::Length(px, pct), true) if px.is_finite() && pct.is_finite() => Some((px, pct)),
        _ => None,
    }
}

/// A calculation's type and value (CSS Values 4 §10.7): a number, or a
/// length as px plus a percentage.
#[derive(Clone, Copy)]
enum Term {
    Number(f64),
    Length(f64, f64),
}

/// The levels of parentheses and functions a calculation may nest, as the
/// kernel's (CSS Values 4 §10.8 leaves the limit to the UA).
const MAX_DEPTH: usize = 32;

/// A recursive-descent reader of `calc()`'s grammar (CSS Values 4 §10.1).
struct Calc<'a> {
    s: &'a [u8],
    at: usize,
    depth: usize,
}

impl Calc<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    /// Skips white space; whether there was any.
    fn space(&mut self) -> bool {
        let start = self.at;
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0c)) {
            self.at += 1;
        }
        self.at > start
    }

    /// `<calc-sum>`: `+` and `-` need white space on both sides.
    fn sum(&mut self) -> Option<Term> {
        let mut a = self.product()?;
        loop {
            let back = self.at;
            let sign = match (self.space(), self.peek()) {
                (true, Some(b'+')) => 1.0,
                (true, Some(b'-')) => -1.0,
                _ => {
                    self.at = back;
                    return Some(a);
                }
            };
            self.at += 1;
            if !self.space() {
                return None;
            }
            a = match (a, self.product()?) {
                (Term::Number(x), Term::Number(y)) => Term::Number(x + sign * y),
                (Term::Length(x, p), Term::Length(y, q)) => {
                    Term::Length(x + sign * y, p + sign * q)
                }
                _ => return None,
            };
        }
    }

    /// `<calc-product>`: a length times or over a number, never two lengths.
    fn product(&mut self) -> Option<Term> {
        let mut a = self.value()?;
        loop {
            let back = self.at;
            self.space();
            let op = match self.peek() {
                Some(op @ (b'*' | b'/')) => op,
                _ => {
                    self.at = back;
                    return Some(a);
                }
            };
            self.at += 1;
            self.space();
            a = match (a, self.value()?, op) {
                (Term::Number(x), Term::Number(y), b'*') => Term::Number(x * y),
                (Term::Number(x), Term::Number(y), _) => Term::Number(x / y),
                (Term::Number(n), Term::Length(x, p), b'*')
                | (Term::Length(x, p), Term::Number(n), b'*') => Term::Length(x * n, p * n),
                (Term::Length(x, p), Term::Number(n), _) => Term::Length(x / n, p / n),
                _ => return None,
            };
        }
    }

    /// `<calc-value>`: a number, a length, a percentage, a parenthesised sum
    /// or a nested `calc()`.
    fn value(&mut self) -> Option<Term> {
        let open = match &self.s[self.at..] {
            [b'(', ..] => 1,
            [a, b, c, d, b'(', ..] if [*a, *b, *c, *d].eq_ignore_ascii_case(b"calc") => 5,
            _ => return self.dimension(),
        };
        if self.depth == MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        self.at += open;
        self.space();
        let v = self.sum()?;
        self.space();
        if self.peek() != Some(b')') {
            return None;
        }
        self.at += 1;
        self.depth -= 1;
        Some(v)
    }

    /// A number, then a unit or `%`. Absolute lengths convert to px (CSS
    /// Values 4 §6.2).
    fn dimension(&mut self) -> Option<Term> {
        let start = self.at;
        let digit = |b: Option<&u8>| b.is_some_and(u8::is_ascii_digit);
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.at += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9' | b'.')) {
            self.at += 1;
        }
        if matches!(self.peek(), Some(b'e' | b'E'))
            && (digit(self.s.get(self.at + 1))
                || (matches!(self.s.get(self.at + 1), Some(b'+' | b'-'))
                    && digit(self.s.get(self.at + 2))))
        {
            self.at += 2;
            while digit(self.s.get(self.at)) {
                self.at += 1;
            }
        }
        let n = word::number(std::str::from_utf8(&self.s[start..self.at]).ok()?)?;
        let unit_start = self.at;
        while matches!(self.peek(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'%')) {
            self.at += 1;
        }
        let unit = std::str::from_utf8(&self.s[unit_start..self.at]).ok()?;
        let px = match unit.to_ascii_lowercase().as_str() {
            "" => return Some(Term::Number(n)),
            "%" => return Some(Term::Length(0.0, n)),
            "px" => 1.0,
            "in" => 96.0,
            "cm" => 96.0 / 2.54,
            "mm" => 96.0 / 25.4,
            "q" => 96.0 / 101.6,
            "pt" => 96.0 / 72.0,
            "pc" => 16.0,
            _ => return None,
        };
        Some(Term::Length(n * px, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_reads_calc_as_px_plus_a_percentage() {
        let four = |x, y, px, py| Some(Value::four(x, y, px, py));
        assert_eq!(translate("10px\t50%", false), four(10.0, 0.0, 0.0, 50.0));
        assert_eq!(
            translate("calc(50% - 12px) calc(-100% + 4px)", false),
            four(-12.0, 4.0, 50.0, -100.0)
        );
        assert_eq!(
            translate("CALC((50% + 1in) / 2) calc(2 * calc(3px - 1px))", false),
            four(48.0, 4.0, 25.0, 0.0)
        );
        assert_eq!(
            translate("calc(10px) 0 5px", true),
            four(10.0, 0.0, 0.0, 0.0)
        );
        // The web's keyframe text for a value reads back as it.
        let v = Value::four(-4.0, 3.0, -50.0, 25.0);
        let css = crate::animation::value_css(crate::Property::Translate, v);
        assert_eq!(translate(&css, false), Some(v));
        for refused in [
            "calc(50% -12px)",
            "calc(50%-12px)",
            "calc(10px * 2px)",
            "calc(1rem + 50%)",
            "calc(2 + 3)",
            "calc(10px",
            "(10px)",
            "min(10px, 50%)",
            "calc(1px / 0)",
            "10px 0 5px",
        ] {
            assert_eq!(translate(refused, false), None, "{refused}");
        }
        assert_eq!(translate("0 0 5%", true), None);
    }
}
