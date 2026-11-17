//! CSS `transition` shorthand → [`Transitions`].
//!
//! @ref LLP 1002 §2, LLP 1115 §2 (names stay CSS: the authored form is CSS's own
//! `transition` shorthand; `-exact-spring(stiffness, damping, mass)` is the
//! one declared extension, LLP 1081 D2)
//!
//! `transition: <property> || <duration> || <easing> || <delay>, …` — each
//! part in CSS's own grammar, with `-exact-spring(k, d, m)` admitted where an easing
//! goes. The first time is the duration and the second is the delay, wherever
//! the other components occur. What CSS's parser would reject, this rejects,
//! by name.

use crate::easing::{Easing, LinearStop, StepPosition};
use crate::word;

use crate::spring::SpringConfig;
use crate::transition::{
    TimingFunction, Transition, TransitionError, TransitionProperty, Transitions,
};

/// Why a shorthand was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// An empty declaration.
    Empty,
    /// A property name outside `all` and the supported [`Property`] names.
    UnknownProperty(String),
    /// A time without `s`/`ms`, or not a number; in a transition, a bare
    /// number.
    BadTime(String),
    /// A transition's part that is neither a time nor an easing this parser
    /// knows (an animation's easing refuses with [`ParseError::BadValue`]).
    BadEasing(String),
    /// A declaration that breaks the shorthand's shape (a third time, a part
    /// given twice, too many parts, a keyframes body out of form): the
    /// sentence that says which.
    BadShape(String),
    /// A known easing function given arguments it does not take.
    BadArguments {
        /// The function as written.
        text: String,
        /// Its name, from [`EASING_FUNCTIONS`].
        function: &'static str,
    },
    /// Parsed, but the evaluator refuses it.
    Invalid(TransitionError),
    /// A value outside its property's grammar (LLP 1055 D6): the sentence
    /// that says which value and what the property takes.
    BadValue(String),
    /// Parsed, but the animation sampler refuses it (LLP 1055 D5).
    InvalidAnimation(crate::animation::AnimationError),
    /// `-exact-spring()` where an animation's easing goes.
    SpringInAnimation(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => f.write_str("the value is empty"),
            ParseError::UnknownProperty(p) => {
                write!(f, "`{p}` is not a property a transition animates")
            }
            ParseError::BadTime(s) => write!(
                f,
                "`{s}` is not a time: a time is a number with `s` or `ms` (`0.3s`, `300ms`)"
            ),
            ParseError::BadEasing(s) => write!(
                f,
                "`{s}` is neither a time (a number with `s` or `ms`) nor an easing: linear, ease, ease-in, ease-out, ease-in-out, step-start, step-end, cubic-bezier(), steps(), linear() or -exact-spring()"
            ),
            ParseError::BadShape(s) => f.write_str(s),
            ParseError::BadArguments { text, function } => {
                let takes = match *function {
                    "cubic-bezier" => "four numbers: x1, y1, x2, y2",
                    "steps" => "a whole number of steps from 1 to 65535, then optionally a position: jump-start, jump-end, jump-none, jump-both, start or end",
                    "linear" => "stops separated by commas, each a number and up to two percentages",
                    _ => "no numbers, or three: stiffness, damping and mass",
                };
                write!(f, "`{text}`: `{function}()` takes {takes}")
            }
            ParseError::Invalid(e) => e.fmt(f),
            ParseError::BadValue(s) => f.write_str(s),
            ParseError::InvalidAnimation(e) => e.fmt(f),
            ParseError::SpringInAnimation(s) => write!(
                f,
                "`{s}`: a spring is a transition's timing function, not an animation's"
            ),
        }
    }
}

impl Transitions {
    /// Parse the CSS shorthand.
    pub fn parse(text: &str) -> Result<Transitions, ParseError> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("none") {
            return Ok(Transitions::NONE);
        }
        let mut out = Vec::new();
        for decl in split_top_level(text, ',') {
            let parts: Vec<&str> = split_top_level(decl.trim(), ' ')
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            let decl = decl.trim();
            if parts.is_empty() {
                return Err(ParseError::BadShape(format!(
                    "`{text}` has an empty entry: each comma separates two transitions"
                )));
            }
            if parts.len() > 4 {
                return Err(ParseError::BadShape(format!("`{decl}` has more than four parts: a transition is a property, a duration, an easing and a delay")));
            }
            let mut property = None;
            let mut duration = 0.0;
            let mut delay = 0.0;
            let mut timing = None;
            let mut times = 0;
            for part in &parts {
                if let Ok(t) = time(part) {
                    match times {
                        0 => duration = t,
                        1 => delay = t,
                        _ => return Err(ParseError::BadShape(format!("`{decl}` has a third time: a transition takes a duration, then a delay"))),
                    }
                    times += 1;
                } else if word::time_like(part) {
                    return Err(ParseError::BadTime((*part).to_string()));
                } else if word::is_number(part) {
                    // A number is never a property or an easing here: a time
                    // written without its unit.
                    return Err(ParseError::BadTime((*part).to_string()));
                } else if let Some(named) =
                    TransitionProperty::from_name(&part.to_ascii_lowercase())
                {
                    if property.is_some() {
                        return Err(ParseError::BadShape(format!("`{decl}` names two properties: a transition names one (`all` for every one)")));
                    }
                    property = Some(named);
                } else {
                    match easing(part) {
                        Ok(value) if timing.is_none() => timing = Some(value),
                        Ok(_) => {
                            return Err(ParseError::BadShape(format!(
                                "`{decl}` has two easings: a transition takes one"
                            )))
                        }
                        Err(ParseError::BadEasing(_)) if property.is_none() => {
                            return Err(ParseError::UnknownProperty((*part).to_string()))
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            let t = Transition {
                property: property.unwrap_or(TransitionProperty::All),
                duration,
                delay,
                timing: timing.unwrap_or(TimingFunction::Easing(Easing::Ease)),
            };
            t.validate().map_err(ParseError::Invalid)?;
            out.push(t);
        }
        let ts = Transitions(out);
        ts.validate().map_err(ParseError::Invalid)?;
        Ok(ts)
    }
}

pub(crate) fn time(s: &str) -> Result<f64, ParseError> {
    word::time(s).ok_or_else(|| ParseError::BadTime(s.to_string()))
}

/// The functions an easing may be, each with its kind (LLP 1081 D8): CSS's,
/// and the spring Exact adds, spelled `-exact-` because WebKit's own
/// `-exact-spring()` takes other arguments in another order.
pub const EASING_FUNCTIONS: &[(&str, &str)] = &[
    ("cubic-bezier", "css CSS Easing 1"),
    ("steps", "css CSS Easing 1"),
    ("linear", "css CSS Easing 2"),
    ("-exact-spring", "exact LLP 1002"),
];

pub(crate) fn easing(s: &str) -> Result<TimingFunction, ParseError> {
    Ok(TimingFunction::Easing(
        match word::keyword(s, &word::EASING_KEYWORDS) {
            Some("linear") => Easing::Linear,
            Some("ease") => Easing::Ease,
            Some("ease-in") => Easing::EaseIn,
            Some("ease-out") => Easing::EaseOut,
            Some("ease-in-out") => Easing::EaseInOut,
            Some("step-start") => Easing::Steps {
                count: 1,
                position: StepPosition::JumpStart,
            },
            Some(_) => Easing::Steps {
                count: 1,
                position: StepPosition::JumpEnd,
            },
            None => {
                let (function, args) =
                    call(s).ok_or_else(|| ParseError::BadEasing(s.to_string()))?;
                let bad = || ParseError::BadArguments {
                    text: s.to_string(),
                    function,
                };
                match function {
                    "cubic-bezier" => {
                        let n = numbers(&args).ok_or_else(bad)?;
                        if n.len() != 4 {
                            return Err(bad());
                        }
                        Easing::CubicBezier {
                            x1: n[0],
                            y1: n[1],
                            x2: n[2],
                            y2: n[3],
                        }
                    }
                    // A count and at most one position (CSS Easing 1 §3.1).
                    "steps" if !(1..=2).contains(&args.len()) => return Err(bad()),
                    "steps" => {
                        let count = args
                            .first()
                            .and_then(|a| a.parse::<u16>().ok())
                            .ok_or_else(bad)?;
                        const POSITIONS: [&str; 6] = [
                            "jump-end",
                            "end",
                            "jump-start",
                            "start",
                            "jump-none",
                            "jump-both",
                        ];
                        let position = match args.get(1).map(|a| word::keyword(a, &POSITIONS)) {
                            None | Some(Some("jump-end" | "end")) => StepPosition::JumpEnd,
                            Some(Some("jump-start" | "start")) => StepPosition::JumpStart,
                            Some(Some("jump-none")) => StepPosition::JumpNone,
                            Some(Some(_)) => StepPosition::JumpBoth,
                            Some(None) => return Err(bad()),
                        };
                        Easing::Steps { count, position }
                    }
                    "linear" => Easing::PiecewiseLinear(linear_stops(&args).ok_or_else(bad)?),
                    _ => {
                        let n = numbers(&args).ok_or_else(bad)?;
                        let config = match n.len() {
                            0 => SpringConfig::default(),
                            3 => SpringConfig {
                                stiffness: n[0],
                                damping: n[1],
                                mass: n[2],
                            },
                            _ => return Err(bad()),
                        };
                        return Ok(TimingFunction::Spring(config));
                    }
                }
            }
        },
    ))
}

/// `linear()`'s stops, `None` when an argument is not one.
fn linear_stops(args: &[String]) -> Option<Vec<LinearStop>> {
    let mut stops: Vec<(Option<f64>, f64)> = Vec::new();
    for arg in args {
        let fields: Vec<&str> = arg.split_whitespace().collect();
        if fields.is_empty() || fields.len() > 3 {
            return None;
        }
        let output = word::number(fields[0])?;
        if fields.len() == 1 {
            stops.push((None, output));
            continue;
        }
        for field in &fields[1..] {
            let input = field
                .strip_suffix('%')
                .and_then(word::number)
                .map(|value| value / 100.0)?;
            stops.push((Some(input), output));
        }
    }
    if stops.is_empty() {
        return None;
    }

    // CSS Easing 2's fixup: default the endpoints, clamp authored positions
    // to the greatest preceding position, then evenly distribute each run of
    // omitted positions between its authored neighbours.
    if stops[0].0.is_none() {
        stops[0].0 = Some(0.0);
    }
    let last = stops.len() - 1;
    if stops[last].0.is_none() {
        stops[last].0 = Some(1.0);
    }
    let mut greatest = f64::NEG_INFINITY;
    for (input, _) in &mut stops {
        if let Some(value) = input {
            if *value < greatest {
                *value = greatest;
            }
            greatest = *value;
        }
    }
    let mut start = 0;
    while start + 1 < stops.len() {
        if stops[start + 1].0.is_some() {
            start += 1;
            continue;
        }
        let end = (start + 2..stops.len())
            .find(|i| stops[*i].0.is_some())
            .expect("the last linear stop has a position");
        let from = stops[start]
            .0
            .expect("the first linear stop has a position");
        let to = stops[end].0.unwrap();
        let width = (end - start) as f64;
        for (offset, stop) in stops[start + 1..end].iter_mut().enumerate() {
            stop.0 = Some(from + (to - from) * (offset + 1) as f64 / width);
        }
        start = end;
    }
    Some(
        stops
            .into_iter()
            .map(|(input, output)| LinearStop {
                input: input.unwrap(),
                output,
            })
            .collect(),
    )
}

/// An easing function's call: the function, by its name in any case
/// ([`word::easing_call`]), and its arguments, each trimmed.
fn call(s: &str) -> Option<(&'static str, Vec<String>)> {
    let function = word::easing_call(s)?;
    let inner = s[function.len() + 1..].strip_suffix(')')?;
    let args = if inner.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(inner, ',')
            .into_iter()
            .map(|a| a.trim().to_string())
            .collect()
    };
    Some((function, args))
}

fn numbers(args: &[String]) -> Option<Vec<f64>> {
    args.iter().map(|a| word::number(a)).collect()
}

/// Split on `sep` outside parentheses.
pub(crate) fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property::Property;

    #[test]
    fn spring_shorthand_refuses_non_finite_derived_coefficients() {
        assert!(Transitions::parse("opacity -exact-spring(100, 1e200, 1)").is_err());
    }

    #[test]
    fn css_shorthand_parses_and_bad_forms_are_refused_by_name() {
        let t = Transitions::parse("opacity 250ms ease-in-out, all 0.5s cubic-bezier(0.4, 0, 0.2, 1) 100ms, translate -exact-spring(180, 12, 1)").unwrap();
        assert_eq!(t.0.len(), 3);
        assert_eq!(t.0[0].duration, 0.25);
        assert_eq!(t.0[0].timing, TimingFunction::Easing(Easing::EaseInOut));
        assert_eq!(t.0[1].delay, 0.1);
        assert!(
            matches!(t.0[1].timing, TimingFunction::Easing(Easing::CubicBezier { x1, .. }) if x1 == 0.4)
        );
        assert!(
            matches!(t.0[2].timing, TimingFunction::Spring(SpringConfig { stiffness, .. }) if stiffness == 180.0)
        );
        assert_eq!(t.0[2].duration, 0.0);
        // A bare duration is `all`; `ease` is the default easing.
        let t = Transitions::parse("0.3s").unwrap();
        assert_eq!(t.0[0].property, TransitionProperty::All);
        assert_eq!(t.0[0].timing, TimingFunction::Easing(Easing::Ease));
        let t = Transitions::parse("ease 1s").unwrap();
        assert_eq!(t.0[0].property, TransitionProperty::All);
        assert_eq!(t.0[0].duration, 1.0);
        assert_eq!(t.0[0].timing, TimingFunction::Easing(Easing::Ease));
        let t = Transitions::parse("linear 200ms").unwrap();
        assert_eq!(t.0[0].property, TransitionProperty::All);
        assert_eq!(t.0[0].duration, 0.2);
        let t = Transitions::parse("1s opacity").unwrap();
        assert_eq!(
            t.0[0].property,
            TransitionProperty::Property(Property::Opacity)
        );
        assert_eq!(t.0[0].duration, 1.0);
        assert_eq!(Transitions::parse("none").unwrap(), Transitions::NONE);
        assert_eq!(
            Transitions::parse("width 1s"),
            Err(ParseError::UnknownProperty("width".into()))
        );
        assert_eq!(
            Transitions::parse("opacity 1"),
            Err(ParseError::BadTime("1".into())),
            "a unitless number is a time without its unit"
        );
        assert_eq!(
            Transitions::parse("0.3 opacity"),
            Err(ParseError::BadTime("0.3".into()))
        );
        assert_eq!(
            Transitions::parse("opacity 1s bounce"),
            Err(ParseError::BadEasing("bounce".into()))
        );
        assert!(matches!(
            Transitions::parse("opacity 1s ease linear"),
            Err(ParseError::BadShape(_))
        ));
        assert!(matches!(
            Transitions::parse("opacity scale 1s"),
            Err(ParseError::BadShape(_))
        ));
        assert!(matches!(
            Transitions::parse("opacity 1s -exact-spring(1,2,3)"),
            Err(ParseError::Invalid(TransitionError::SpringDeclaresDuration))
        ));
        assert!(matches!(
            Transitions::parse("opacity 1s steps(0)"),
            Err(ParseError::Invalid(_))
        ));
        let steps = Transitions::parse("opacity 1s steps(4, jump-both)").unwrap();
        assert!(matches!(
            steps.0[0].timing,
            TimingFunction::Easing(Easing::Steps {
                count: 4,
                position: StepPosition::JumpBoth
            })
        ));
        let lin = Transitions::parse("opacity 1s linear(0, 0.9 50%, 1)").unwrap();
        let TimingFunction::Easing(Easing::PiecewiseLinear(stops)) = &lin.0[0].timing else {
            panic!()
        };
        assert_eq!(stops.len(), 3);
        assert_eq!((stops[1].input, stops[1].output), (0.5, 0.9));

        let lin = Transitions::parse("opacity 1s linear(0, 0.2, 0.6 60%, 0.8, 1)").unwrap();
        let TimingFunction::Easing(Easing::PiecewiseLinear(stops)) = &lin.0[0].timing else {
            panic!()
        };
        assert_eq!(
            stops
                .iter()
                .map(|stop| (stop.input, stop.output))
                .collect::<Vec<_>>(),
            [(0.0, 0.0), (0.3, 0.2), (0.6, 0.6), (0.8, 0.8), (1.0, 1.0)]
        );
        let lin = Transitions::parse("opacity 1s linear(0 0% 20%, 1 80% 100%)").unwrap();
        let TimingFunction::Easing(Easing::PiecewiseLinear(stops)) = &lin.0[0].timing else {
            panic!()
        };
        assert_eq!(
            stops
                .iter()
                .map(|stop| (stop.input, stop.output))
                .collect::<Vec<_>>(),
            [(0.0, 0.0), (0.2, 0.0), (0.8, 1.0), (1.0, 1.0)]
        );
    }

    #[test]
    fn a_refusal_states_the_rule_in_words() {
        for (text, says) in [
            (
                "opacity 0.3",
                "`0.3` is not a time: a time is a number with `s` or `ms`",
            ),
            (
                "opacity 1s bounce",
                "`bounce` is neither a time (a number with `s` or `ms`) nor an easing: linear,",
            ),
            ("opacity -1s", "a duration is 0 or more"),
            (
                "opacity 1s ease linear",
                "has two easings: a transition takes one",
            ),
            (
                "opacity 0.3s 0.1s 0.2s",
                "has a third time: a transition takes a duration, then a delay",
            ),
            ("opacity scale 1s", "names two properties"),
            (
                "opacity 1s, ",
                "has an empty entry: each comma separates two transitions",
            ),
            (
                "opacity 1s ease 0s opacity linear",
                "has more than four parts",
            ),
            (
                "opacity cubic-bezier(0.4, 0, 0.2)",
                "`cubic-bezier()` takes four numbers: x1, y1, x2, y2",
            ),
            (
                "opacity 1s steps(1.5)",
                "`steps()` takes a whole number of steps",
            ),
            (
                "opacity 1s steps(4, jump-sideways)",
                "then optionally a position: jump-start,",
            ),
            ("opacity 1s linear(x)", "`linear()` takes stops"),
            (
                "opacity -exact-spring(1, 2)",
                "`-exact-spring()` takes no numbers, or three",
            ),
            (
                "cubic-bezier(1) opacity",
                "`cubic-bezier()` takes four numbers",
            ),
            // A word Rust reads as a number is no CSS number: no unit is missing.
            ("opacity inf", "`inf` is neither a time"),
            ("opacity NaN", "`NaN` is neither a time"),
            (
                "opacity .5",
                "`.5` is not a time: a time is a number with `s` or `ms`",
            ),
            ("opacity 1e2", "`1e2` is not a time"),
            ("opacity 1.2.3s", "`1.2.3s` is not a time"),
            ("opacity 1e999s", "a transition's duration is finite"),
            ("opacity 0s 1e999s", "a transition's delay is finite"),
            (
                "opacity -exact-spring() 0.2s",
                "write no time, or `0s` and then the delay",
            ),
            (
                "opacity -exact-spring(1e999, 1, 1)",
                "a spring's stiffness is finite",
            ),
            (
                "opacity -exact-spring(100, 1e200, 1)",
                "use a larger mass, or a smaller stiffness or damping",
            ),
            ("opacity 1s steps(0)", "`steps()` takes at least one step"),
            (
                "opacity 1s cubic-bezier(2, 0, 0.2, 1)",
                "control point's x is from 0 to 1",
            ),
            (
                "opacity 1s -exact-spring(1, 2, 3)",
                "write no time, or `0s` and then the delay",
            ),
            ("opacity -exact-spring(0, 2, 3)", "stiffness is above 0"),
            (
                "width 1s",
                "`width` is not a property a transition animates",
            ),
        ] {
            let message = Transitions::parse(text).unwrap_err().to_string();
            assert!(message.contains(says), "{text}: {message}");
        }
    }

    /// #49: a unit, a keyword, a function's name and a property's name match
    /// in any ASCII case, as CSS's do (CSS Values 4 §2.1, §6.1); a refusal
    /// names the word as written.
    #[test]
    fn a_transition_reads_in_any_ascii_case() {
        for text in [
            "opacity 0.3S EASE",
            "OPACITY 300MS Ease-In-Out 0.1S",
            "Opacity 0.3s Cubic-Bezier(0.4, 0, 0.2, 1)",
            "opacity 0.3s STEPS(4, Jump-Both)",
            "opacity 0.3s steps(2, START)",
            "ALL 0.3s Step-Start",
            "Background-Color 0.3s LINEAR(0, 0.5 50%, 1)",
            "Scale -EXACT-SPRING(180, 12, 1)",
            "-Exact-Tint-Color 1s, Border-Color 2E2MS",
            "NONE",
            "None",
        ] {
            let parsed = Transitions::parse(text);
            assert!(parsed.is_ok(), "{text}: {parsed:?}");
            assert_eq!(
                parsed,
                Transitions::parse(&text.to_ascii_lowercase()),
                "{text}"
            );
        }
        for (text, refused) in [
            (
                "opacity 0.3S Bounce",
                ParseError::BadEasing("Bounce".into()),
            ),
            ("opacity 0.3", ParseError::BadTime("0.3".into())),
            ("opacity 1.S", ParseError::BadTime("1.S".into())),
            ("Width 1S", ParseError::UnknownProperty("Width".into())),
            // Rust reads `inf` as a number; CSS does not, so no time is infinite.
            ("opacity INFS", ParseError::BadEasing("INFS".into())),
        ] {
            assert_eq!(Transitions::parse(text), Err(refused), "{text}");
        }
        assert!(Transitions::parse("opacity 1s STEPS(4, Sideways)")
            .unwrap_err()
            .to_string()
            .starts_with("`STEPS(4, Sideways)`: `steps()` takes"));
    }
}
