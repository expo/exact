//! `input type="number"` on the painted hosts: HTML's Number state
//! (§4.10.5.1.12), whose value is a valid floating-point number or empty.
//! ArrowUp and ArrowDown step it as `stepUp()` and `stepDown()` do, and
//! typed, pasted or driven text keeps only what a number's editor takes, as
//! Chrome's does. The rules are the Apple hosts' (`NumberStepping.swift`).
use super::*;
use exact_kernel::PropList;
use exact_runner::{FieldSelection, InputDetails};

/// A number field's bounds and step.
pub(super) struct Number {
    min: Option<f64>,
    max: Option<f64>,
    step: f64,
}

/// HTML's valid floating-point number, read from a field's text: digits
/// with an optional sign, fraction and exponent. A trailing point (`1.`, on
/// its way to `1.5`) is the number before it; anything else that is no
/// number reads as none, as the web's field holds nothing for it.
fn parse(text: &str) -> Option<f64> {
    let t = text.trim().replace('\u{2212}', "-");
    let t = t.strip_suffix('.').unwrap_or(&t);
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['-', '+']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    let ok = digits(whole)
        && digits(fraction)
        && (!whole.is_empty() || !fraction.is_empty())
        && (!whole.is_empty() || mantissa.contains('.'))
        && exponent_ok;
    ok.then(|| t.parse::<f64>().ok())
        .flatten()
        .filter(|n| n.is_finite())
}

/// A number as the field shows it: the shortest decimal, `3` and never
/// `3.0`, with the float's noise past the ninth decimal place gone (0.1 and
/// 0.2 make 0.3), as the kernel tidies a range's value.
fn text(value: f64) -> String {
    let tidy = (value * 1e9).round() / 1e9;
    format!("{}", if tidy.is_finite() { tidy } else { value } + 0.0)
}

/// `k` as a whole number when it is one but for the float's noise.
fn whole(k: f64) -> Option<f64> {
    ((k - k.round()).abs() < 1e-9).then_some(k.round())
}

impl Number {
    /// The field's rules, when it is a number field.
    pub(super) fn of(props: &PropList) -> Option<Number> {
        if props.str(PropId::Type) != Some("number") {
            return None;
        }
        let min = props.str(PropId::Min).and_then(parse);
        let max =
            (props.str(PropId::Max).and_then(parse)).map(|max| min.map_or(max, |m| max.max(m)));
        let step = (props.str(PropId::Step).and_then(parse))
            .filter(|s| *s > 0.0)
            .unwrap_or(1.0);
        Some(Number { min, max, step })
    }

    /// The field's text after a step up or down, from the step base (`min`,
    /// else 0) and within `min` and `max`; an empty field, or one that holds
    /// no number, steps from 0. `None` when stepping changes nothing: the
    /// value is at the bound it steps toward, or past it on the side it
    /// would move away from (HTML's `stepUp()`, step 10).
    pub(super) fn stepped(&self, field: &str, up: bool) -> Option<String> {
        let parsed = parse(field);
        let before = parsed.unwrap_or(0.0);
        let base = self.min.unwrap_or(0.0);
        let k = (before - base) / self.step;
        let mut value = match whole(k) {
            Some(_) if up => before + self.step,
            Some(_) => before - self.step,
            // Off the step: to the nearest value on it in the direction.
            None if up => base + k.ceil() * self.step,
            None => base + k.floor() * self.step,
        };
        let on = |n: f64, up: bool| {
            let k = (n - base) / self.step;
            let k = whole(k).unwrap_or(if up { k.ceil() } else { k.floor() });
            base + k * self.step
        };
        if let Some(lo) = self.min.filter(|lo| value < *lo) {
            value = on(lo, true);
        }
        if let Some(hi) = self.max.filter(|hi| value > *hi) {
            value = on(hi, false);
        }
        let value = parse(&text(value)).unwrap_or(value);
        if parsed.is_some() && (if up { value <= before } else { value >= before }) {
            return None;
        }
        Some(text(value))
    }
}

/// What of `replacement` a number field takes at `start..end` (byte
/// offsets) of its `field`: digits, a sign where one may stand (anywhere
/// before an exponent; right after its `e`, once), one point before any
/// exponent, and one exponent, as Chrome's editor takes them. Full-width
/// forms count as their ASCII ones.
pub(super) fn insertion(replacement: &str, field: &str, start: usize, end: usize) -> String {
    let remainder = format!("{}{}", &field[..start], &field[end..]);
    let mut point = remainder.contains('.');
    let mut exponent = remainder.to_ascii_lowercase().contains('e');
    let mut prefix = field[..start].to_ascii_lowercase();
    let suffix = field[end..].to_ascii_lowercase();
    let mut taken = String::new();
    for c in replacement.chars() {
        let c = match c {
            '\u{2212}' => '-',
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            c => c,
        };
        match c {
            '0'..='9' => {}
            '+' | '-' => {
                if let Some(e) = prefix.find('e') {
                    let tail = format!("{}{suffix}", &prefix[e + 1..]);
                    if !prefix.ends_with('e') || tail.contains(['+', '-']) {
                        continue;
                    }
                }
            }
            '.' if !point && !prefix.contains('e') => point = true,
            'e' | 'E' if !exponent => exponent = true,
            _ => continue,
        }
        taken.push(c);
        prefix.push(c.to_ascii_lowercase());
    }
    taken
}

impl<D: DataSource> Presenter<D> {
    /// A step of number field `id` showing `value`: its text one step up or
    /// down, then HTML's `input` and `change`, as Chromium's spin buttons
    /// commit each step and the Apple hosts' steppers do. Nothing at a bound.
    pub(super) fn step_number(
        &mut self,
        id: ViewId,
        number: &Number,
        value: &str,
        up: bool,
        now_ms: f64,
    ) {
        let Some(next) = number.stepped(value, up) else {
            return;
        };
        let caret = FieldSelection::caret(field::utf16_len(&next));
        let bound = self.bound_text(id);
        let edit = InputDetails {
            input_type: String::new(),
            data: None,
            cancelable: false,
            is_composing: false,
        };
        if let Some(e) = self.apply_field_edit(id, next, caret, bound, now_ms, edit) {
            eprintln!("exact: {e}");
        }
        if let Some(Some(e)) = self.commit_text(id, now_ms) {
            eprintln!("exact: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: &str = r#"component App
  state port = "443"
  state log = ""
  action edit(v: string)
    port = v
    log = `${log}i${v} `
  action done(v: string)
    log = `${log}c${v} `
  view
    column
      input type="number" testId="port" value=port input=edit change=done min=1 max=65535 step=1 width=120
"#;

    #[test]
    fn a_number_field_steps_on_its_arrows_and_refuses_letters() {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(PORT),
            (),
            (300., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let k = p.host.kernel();
        let id = k.node_by_key(k.find_by_test_id("port")[0]).unwrap().id;
        let log = |p: &Presenter<()>| {
            let log = p.host.runner().slot("log").unwrap();
            log.as_str().unwrap().to_owned()
        };
        assert!(p.set_focus(Some(id), p.host.now()).is_none());
        let mut shown = Vec::new();
        let mut key = |p: &mut Presenter<()>, name: &str| {
            p.key_down(name, p.host.now());
            shown.push(p.field_text(id));
        };
        key(&mut p, "ArrowUp");
        assert_eq!(log(&p), "i444 c444 ", "a step is an input and a change");
        key(&mut p, "ArrowDown");
        key(&mut p, "a");
        p.type_text(id, "4a4").unwrap();
        let typed = p.field_text(id);
        p.type_text(id, "").unwrap();
        key(&mut p, "ArrowUp");
        p.type_text(id, "65535").unwrap();
        key(&mut p, "ArrowUp");
        assert_eq!(shown, ["444", "443", "443", "1", "65535"]);
        assert_eq!(typed, "44", "the agent's text, as a person's typing");
    }

    fn number(min: Option<f64>, max: Option<f64>, step: f64) -> Number {
        Number { min, max, step }
    }

    #[test]
    fn steps_as_html_steps_up_and_down() {
        let port = number(Some(1.0), Some(65535.0), 1.0);
        assert_eq!(port.stepped("443", true).as_deref(), Some("444"));
        assert_eq!(port.stepped("443", false).as_deref(), Some("442"));
        assert_eq!(
            port.stepped("", true).as_deref(),
            Some("1"),
            "empty steps from 0, to min"
        );
        assert_eq!(port.stepped("65535", true), None, "at max");
        assert_eq!(port.stepped("1", false), None, "at min");
        assert_eq!(port.stepped("70000", false).as_deref(), Some("65535"));
        assert_eq!(
            port.stepped("70000", true),
            None,
            "past max, up moves nowhere"
        );
        let tenth = number(None, None, 0.1);
        assert_eq!(tenth.stepped("0.2", true).as_deref(), Some("0.3"));
        assert_eq!(
            tenth.stepped("0.25", true).as_deref(),
            Some("0.3"),
            "off the step"
        );
        assert_eq!(tenth.stepped("0.25", false).as_deref(), Some("0.2"));
        assert_eq!(tenth.stepped("0.1", false).as_deref(), Some("0"));
        assert_eq!(
            number(None, None, 1.0).stepped("1e3", true).as_deref(),
            Some("1001")
        );
        let odd = number(Some(1.0), Some(10.0), 2.0);
        assert_eq!(
            odd.stepped("4", true).as_deref(),
            Some("5"),
            "from the step base"
        );
        assert_eq!(
            odd.stepped("9", true),
            None,
            "11 is past max; 9 is the last on the step"
        );
    }

    #[test]
    fn reads_html_numbers_only() {
        assert_eq!(parse("1."), Some(1.0));
        assert_eq!(parse("-1.5e+2"), Some(-150.0));
        assert_eq!(parse(".5"), Some(0.5));
        for none in ["", "a", "1e", "--1", "1.2.3", ".", "e5", "inf", "NaN"] {
            assert_eq!(parse(none), None, "{none:?}");
        }
    }

    #[test]
    fn takes_what_chromes_number_editor_takes() {
        let typed = |field: &str, at: usize, text: &str| insertion(text, field, at, at);
        assert_eq!(typed("443", 3, "a"), "");
        assert_eq!(insertion("4a4", "443", 0, 3), "44");
        assert_eq!(typed("12", 2, "-"), "-", "Chrome keeps an incomplete sign");
        assert_eq!(typed("1e2", 3, "-"), "", "not after the exponent's digits");
        assert_eq!(typed("1e", 2, "-"), "-");
        assert_eq!(typed("1.5", 3, "."), "", "one point");
        assert_eq!(typed("1e5", 1, "."), ".", "a point before the exponent");
        assert_eq!(typed("1e5", 3, "e"), "", "one exponent");
        assert_eq!(typed("", 0, "\u{ff11}\u{ff12}"), "12", "full-width digits");
    }
}
