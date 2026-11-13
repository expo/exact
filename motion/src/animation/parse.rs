//! The `animation` shorthand and `@keyframes` bodies, in CSS's grammar.
//!
//! @ref LLP 1055 D5; CSS Animations 1 §3 (`@keyframes`), §4.10 (the shorthand)
//!
//! `animation: <name> || <duration> || <easing> || <delay> || <count> ||
//! <direction> || <fill-mode> || <play-state>, …` — the first time is the
//! duration and the second the delay; a keyword that could belong to another
//! longhand goes there first, and the name is what is left. A keyframes body
//! is `from{r:3;opacity:.5}50%,75%{…}to{…}`, the form Contract's `keyframes`
//! declaration lowers to and the web host emits.

use super::{Animation, AnimationError, Animations, Direction, FillMode, Keyframe, Keyframes};
use crate::easing::Easing;
use crate::parse::{easing, split_top_level, time, ParseError};
use crate::property::{Property, Value};
use crate::transition::TimingFunction;
use crate::word;
use exact_num::Shortest;
use std::fmt::Write as _;

/// A grammar: the value, or why CSS refuses the text.
type Grammar<T> = fn(&str) -> Result<T, ParseError>;

/// The two grammars a link registers.
struct Grammars {
    animations: Grammar<Animations>,
    keyframes: Grammar<Keyframes>,
}

const GRAMMARS: Grammars = Grammars {
    animations: Animations::grammar,
    keyframes: Keyframes::grammar,
};

/// The grammars, once linked ([`link`]). Only a wasm artifact reads it.
#[cfg(target_arch = "wasm32")]
static LINKED: std::sync::OnceLock<&'static Grammars> = std::sync::OnceLock::new();

/// Link the `animation` shorthand's and `@keyframes`' grammars (LLP 1047
/// D2, linked by use): ~5 KiB of a web core that none of Caltrain, RealWorld
/// or the video player animates with. A web artifact links them when its
/// plan declares keyframes or binds `animation` or `-exact-exit-animation`, and a
/// plan that does so unlinked is refused at boot (D6), so an unlinked
/// artifact never parses one. Native artifacts and the compiler parse
/// without it.
pub fn link() {
    #[cfg(target_arch = "wasm32")]
    let _ = LINKED.set(&GRAMMARS);
}

/// `text` by the grammar `pick` names, on the web once linked.
fn parse_linked<T>(text: &str, pick: fn(&Grammars) -> Grammar<T>) -> Result<T, ParseError> {
    #[cfg(target_arch = "wasm32")]
    return LINKED.get().map_or_else(
        || {
            Err(ParseError::BadShape(
                "CSS animations are not linked into this artifact (LLP 1047 D6)".into(),
            ))
        },
        |grammars| pick(grammars)(text),
    );
    #[cfg(not(target_arch = "wasm32"))]
    pick(&GRAMMARS)(text)
}

impl Animations {
    /// Parse the shorthand. Names stay unresolved (no keyframes) until
    /// [`Animations::resolve`]. On the web, once linked ([`link`]).
    pub fn parse(text: &str) -> Result<Animations, ParseError> {
        parse_linked(text, |g| g.animations)
    }

    fn grammar(text: &str) -> Result<Animations, ParseError> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("none") {
            return Ok(Animations::NONE);
        }
        let mut out = Vec::new();
        for decl in split_top_level(text, ',') {
            let decl = decl.trim();
            let parts: Vec<&str> = split_top_level(decl, ' ')
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            if parts.is_empty() {
                return Err(ParseError::BadShape(format!(
                    "`{text}` has an empty entry: each comma separates two animations"
                )));
            }
            if parts.len() > 8 {
                return Err(ParseError::BadShape(format!("`{decl}` has more than eight parts: an animation is a name, a duration, an easing, a delay, an iteration count, a direction, a fill mode and a play state")));
            }
            let mut a = Animation::default();
            let (mut slots, mut named) = (Slots::default(), false);
            for part in parts {
                let delay = slots.times > 0;
                match slots.take(part) {
                    Some(Role::Time) if delay => a.delay = time(part)?,
                    Some(Role::Time) => a.duration = time(part)?,
                    Some(Role::Easing) => a.easing = easing_only(part)?,
                    Some(Role::Count) => a.iterations = count(part).unwrap(),
                    Some(Role::Direction) => a.direction = direction(part).unwrap(),
                    Some(Role::Fill) => a.fill = fill(part).unwrap(),
                    Some(Role::Play) => a.paused = play_state(part).unwrap(),
                    Some(Role::Name) => {
                        a.name = held_name(part);
                        named = true;
                    }
                    None if time(part).is_ok() => {
                        return Err(ParseError::BadShape(format!(
                        "`{decl}` has a third time: an animation takes a duration, then a delay"
                    )))
                    }
                    None if word::time_like(part) => {
                        return Err(ParseError::BadTime(part.to_string()))
                    }
                    None => return Err(ParseError::BadShape(left_over(part, decl, &a.name))),
                }
            }
            a.validate().map_err(ParseError::InvalidAnimation)?;
            if named && a.name != "none" {
                out.push(a);
            }
        }
        if out.len() > super::MAX_ANIMATIONS {
            return Err(ParseError::InvalidAnimation(AnimationError::TooMany));
        }
        Ok(Animations(out))
    }

    /// Give each entry the keyframes its name resolves to. An entry whose
    /// name matches no rule is dropped: CSS starts no animation for it
    /// (CSS Animations 1 §3). Returns the names dropped.
    pub fn resolve<'k>(
        &mut self,
        mut lookup: impl FnMut(&str) -> Option<&'k Keyframes>,
    ) -> Vec<String> {
        let mut dropped = Vec::new();
        self.0.retain_mut(|a| match lookup(&a.name) {
            Some(k) => {
                a.keyframes = k.clone();
                true
            }
            None => {
                dropped.push(std::mem::take(&mut a.name));
                false
            }
        });
        dropped
    }

    /// The shorthand, every longhand spelled out, as CSS reads it.
    pub fn css(&self) -> String {
        self.css_named(&|a| std::borrow::Cow::Borrowed(&a.name))
    }

    /// [`Animations::css`], each entry naming the rule `name` gives it (a
    /// host's variant of a rule, say).
    pub fn css_named(&self, name: &dyn Fn(&Animation) -> std::borrow::Cow<'_, str>) -> String {
        if self.0.is_empty() {
            return "none".into();
        }
        let mut out = String::new();
        for (i, a) in self.0.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let _ = write!(
                out,
                "{}s {} {}s ",
                Shortest(a.duration),
                easing_css(&a.easing),
                Shortest(a.delay)
            );
            if a.iterations.is_infinite() {
                out.push_str("infinite");
            } else {
                let _ = write!(out, "{}", Shortest(a.iterations));
            }
            let _ = write!(
                out,
                " {} {} {} {}",
                a.direction.name(),
                a.fill.name(),
                if a.paused { "paused" } else { "running" },
                css_ident(&name(a))
            );
        }
        out
    }
}

/// The eight longhands `animation` sets, in CSS's order.
pub const LONGHANDS: [&str; 8] = [
    "animation-name",
    "animation-duration",
    "animation-timing-function",
    "animation-delay",
    "animation-iteration-count",
    "animation-direction",
    "animation-fill-mode",
    "animation-play-state",
];

impl Animation {
    /// Set one longhand from one item of its list (CSS Animations 1 §4).
    pub fn set_longhand(&mut self, name: &str, value: &str) -> Result<(), ParseError> {
        let value = value.trim();
        let bad = || {
            ParseError::BadValue(match name {
                "animation-name" => format!("`{value}` is not a keyframes name: a name is an identifier other than `initial`, `inherit`, `unset`, `revert`, `revert-layer` and `default`, or a quoted string"),
                "animation-iteration-count" => format!("`{value}` is not an iteration count: a number 0 or more, or `infinite`"),
                "animation-direction" => format!("`{value}` is not a direction: normal, reverse, alternate or alternate-reverse"),
                "animation-fill-mode" => format!("`{value}` is not a fill mode: none, forwards, backwards or both"),
                "animation-play-state" => format!("`{value}` is not a play state: running or paused"),
                _ => format!("`{name}` is not an animation longhand"),
            })
        };
        match name {
            "animation-name" if is_name(value) => self.name = held_name(value),
            "animation-duration" => self.duration = time(value)?,
            "animation-delay" => self.delay = time(value)?,
            "animation-timing-function" => self.easing = easing_only(value)?,
            "animation-iteration-count" => self.iterations = count(value).ok_or_else(bad)?,
            "animation-direction" => self.direction = direction(value).ok_or_else(bad)?,
            "animation-fill-mode" => self.fill = fill(value).ok_or_else(bad)?,
            "animation-play-state" => self.paused = play_state(value).ok_or_else(bad)?,
            _ => return Err(bad()),
        }
        self.validate().map_err(ParseError::InvalidAnimation)
    }
}

impl Keyframes {
    /// Parse a keyframes body: `<selectors>{<property>:<value>;…}…`. Keyframes
    /// sharing an offset and timing function merge (a later value wins); the
    /// result is in offset order, stable for equal offsets.
    /// On the web, once linked ([`link`]).
    pub fn parse(text: &str) -> Result<Keyframes, ParseError> {
        parse_linked(text, |g| g.keyframes)
    }

    fn grammar(text: &str) -> Result<Keyframes, ParseError> {
        let mut frames: Vec<Keyframe> = Vec::new();
        let mut rest = text.trim();
        while !rest.is_empty() {
            let open = rest
                .find('{')
                .ok_or_else(|| {
                    ParseError::BadShape(format!("`{rest}` has no `{{`: a keyframe is its selectors, then `{{property: value; …}}`"))
                })?;
            let close = rest[open..]
                .find('}')
                .map(|i| open + i)
                .ok_or_else(|| ParseError::BadShape(format!("`{rest}` has no closing `}}`")))?;
            let selectors = &rest[..open];
            let body = &rest[open + 1..close];
            let mut easing_here = None;
            let mut values: Vec<(Property, Value)> = Vec::new();
            let mut dark: Vec<(Property, Value)> = Vec::new();
            for decl in split_top_level(body, ';')
                .into_iter()
                .map(str::trim)
                .filter(|d| !d.is_empty())
            {
                let (name, value) = decl.split_once(':').ok_or_else(|| {
                    ParseError::BadShape(format!("`{decl}` in a keyframe is not `property: value`"))
                })?;
                let (name, value) = (name.trim(), value.trim());
                if name == "animation-timing-function" {
                    easing_here = Some(easing_only(value)?);
                    continue;
                }
                let property = Property::from_name(name)
                    .filter(|p| *p != Property::Height)
                    .ok_or_else(|| {
                        ParseError::BadValue(format!(
                            "`{name}` is not a property keyframes animate"
                        ))
                    })?;
                let declared = keyframe_values(property, value)?;
                // A later declaration replaces the whole value: every axis
                // of a `font-variation-settings`, both halves of a shadow.
                for (property, _, _) in &declared {
                    values.retain(|(p, _)| p != property);
                    dark.retain(|(p, _)| p != property);
                }
                for (property, light, night) in declared {
                    values.push((property, light));
                    if let Some(night) = night {
                        dark.push((property, night));
                    }
                }
            }
            for selector in selectors.split(',').map(str::trim) {
                let offset = match word::keyword(selector, &["from", "to"]) {
                    Some("from") => 0.0,
                    Some(_) => 1.0,
                    None => selector
                        .strip_suffix('%')
                        .and_then(|n| exact_num::parse_f64(n.trim()).ok())
                        .filter(|n| (0.0..=100.0).contains(n))
                        .map(|n| n / 100.0)
                        .ok_or_else(|| {
                            ParseError::BadShape(format!("`{selector}` is not a keyframe selector: `from`, `to` or a percentage from 0% to 100%"))
                        })?,
                };
                match frames
                    .iter_mut()
                    .find(|f| f.offset == offset && f.easing == easing_here)
                {
                    Some(frame) => {
                        for (p, _) in &values {
                            frame.values.retain(|(q, _)| q != p);
                            frame.dark.retain(|(q, _)| q != p);
                        }
                        frame.values.extend(values.iter().copied());
                        frame.dark.extend(dark.iter().copied());
                    }
                    None => frames.push(Keyframe {
                        offset,
                        easing: easing_here.clone(),
                        values: values.clone(),
                        dark: dark.clone(),
                    }),
                }
            }
            rest = rest[close + 1..].trim_start();
        }
        frames.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let keyframes = Keyframes(frames.into());
        keyframes.validate().map_err(ParseError::InvalidAnimation)?;
        Ok(keyframes)
    }

    /// The body as CSS (percentages, CSS units), for a web host's
    /// `@keyframes name { … }`.
    pub fn css(&self) -> String {
        let mut out = String::new();
        for frame in self.0.iter() {
            let _ = write!(out, "{}%{{", Shortest(frame.offset * 100.0));
            if let Some(e) = &frame.easing {
                let _ = write!(out, "animation-timing-function:{};", easing_css(e));
            }
            // The axes are one `font-variation-settings`, where its first is.
            let axes: Vec<String> = (frame.values.iter())
                .filter(|(p, _)| *p == Property::FontVariationSettings)
                .map(|(p, v)| value_css(*p, *v))
                .collect();
            let mut axes = Some(axes.join(","));
            // The shadow's colour is written in its `box-shadow`.
            for (p, v) in frame
                .values
                .iter()
                .filter(|(p, _)| *p != Property::ShadowColor)
            {
                if *p == Property::FontVariationSettings {
                    if let Some(axes) = axes.take() {
                        let _ = write!(out, "font-variation-settings:{axes};");
                    }
                    continue;
                }
                let colour = |p: Property, v: Value| match frame.dark.iter().find(|(q, _)| *q == p)
                {
                    Some((_, night)) => format!(
                        "light-dark({}, {})",
                        crate::color::css(v),
                        crate::color::css(*night)
                    ),
                    None => crate::color::css(v),
                };
                let text = match p {
                    p if p.is_color() => colour(*p, *v),
                    Property::BoxShadow => {
                        let shade = frame
                            .values
                            .iter()
                            .find(|(q, _)| *q == Property::ShadowColor)
                            .map_or(Value::ZERO, |(_, c)| *c);
                        format!(
                            "{} {}",
                            value_css(*p, *v),
                            colour(Property::ShadowColor, shade)
                        )
                    }
                    _ => value_css(*p, *v),
                };
                let _ = write!(out, "{}:{};", p.css_name(), text);
            }
            out.push('}');
        }
        out
    }
}

/// A property value in CSS, with the unit CSS requires.
pub fn value_css(property: Property, v: Value) -> String {
    match property {
        Property::Translate => {
            let axis = |px: f64, pct: f64| match (px, pct) {
                (_, 0.0) => format!("{}px", Shortest(px)),
                (0.0, _) => format!("{}%", Shortest(pct)),
                _ => format!("calc({}px + {}%)", Shortest(px), Shortest(pct)),
            };
            format!("{} {}", axis(v.x, v.z), axis(v.y, v.w))
        }
        Property::Rotate => format!("{}deg", Shortest(v.x)),
        Property::R
        | Property::Height
        | Property::Cx
        | Property::Cy
        | Property::X
        | Property::Y
        | Property::Rx
        | Property::Ry => format!("{}px", Shortest(v.x)),
        Property::Scale | Property::Opacity | Property::StrokeDashoffset | Property::FontWeight => {
            format!("{}", Shortest(v.x))
        }
        Property::FontStretch => format!("{}%", Shortest(v.x)),
        Property::FontVariationSettings => {
            let tag: String = v.axis_tag().to_be_bytes().map(char::from).iter().collect();
            format!("\"{tag}\" {}", Shortest(v.x))
        }
        // Offset and blur; the colour is `box-shadow`'s other half.
        Property::BoxShadow => format!(
            "{}px {}px {}px",
            Shortest(v.x),
            Shortest(v.y),
            Shortest(v.z)
        ),
        Property::Layout => format!(
            "{}px {}px {}px {}px",
            Shortest(v.x),
            Shortest(v.y),
            Shortest(v.z),
            Shortest(v.w)
        ),
        _ => crate::color::css(v),
    }
}

/// An easing in CSS.
pub fn easing_css(e: &Easing) -> String {
    match e {
        Easing::Linear => "linear".into(),
        Easing::Ease => "ease".into(),
        Easing::EaseIn => "ease-in".into(),
        Easing::EaseOut => "ease-out".into(),
        Easing::EaseInOut => "ease-in-out".into(),
        Easing::CubicBezier { x1, y1, x2, y2 } => format!(
            "cubic-bezier({}, {}, {}, {})",
            Shortest(*x1),
            Shortest(*y1),
            Shortest(*x2),
            Shortest(*y2)
        ),
        Easing::Steps { count, position } => format!("steps({count}, {})", position.name()),
        Easing::PiecewiseLinear(stops) => {
            let parts: Vec<String> = stops
                .iter()
                .map(|s| format!("{} {}%", Shortest(s.output), Shortest(s.input * 100.0)))
                .collect();
            format!("linear({})", parts.join(", "))
        }
    }
}

fn css_ident(name: &str) -> String {
    if name.is_empty() || !is_name(name) {
        format!("\"{}\"", name.replace('"', "\\\""))
    } else {
        name.to_string()
    }
}

/// What a word of an `animation` shorthand gives (CSS Animations 1 §4.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// The duration, then the delay.
    Time,
    /// `animation-timing-function`.
    Easing,
    /// `animation-iteration-count`.
    Count,
    /// `animation-direction`.
    Direction,
    /// `animation-fill-mode`.
    Fill,
    /// `animation-play-state`.
    Play,
    /// `animation-name`: a keyframes name, kept as written.
    Name,
}

/// The parts one animation of a shorthand has been given so far. A word
/// gives the first part still free that it can be, in [`Role`]'s order, so a
/// keyword whose part is given is the name (CSS Animations 1 §4.10). This
/// grammar reads a shorthand with it, and so does the module-scope pass that
/// renames a keyframes name where it is written (contract-syntax's
/// `scope.rs`, LLP 1091 D5): the two cannot read a word differently.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Slots {
    times: u8,
    eased: bool,
    counted: bool,
    directed: bool,
    filled: bool,
    stated: bool,
    named: bool,
}

impl Slots {
    /// The part `word` gives, now taken; `None` where the shorthand is
    /// refused: a third time, a word written as a time that is none
    /// (`1.2.3s`), or a word no free part takes.
    pub fn take(&mut self, word: &str) -> Option<Role> {
        let role = if word::time(word).is_some() {
            Role::Time
        } else if word::time_like(word) {
            return None;
        } else if !self.eased && is_easing(word) {
            Role::Easing
        } else if !self.counted && count(word).is_some() {
            Role::Count
        } else if !self.directed && direction(word).is_some() {
            Role::Direction
        } else if !self.filled && fill(word).is_some() {
            Role::Fill
        } else if !self.stated && play_state(word).is_some() {
            Role::Play
        } else if !self.named && is_name(word) {
            Role::Name
        } else {
            return None;
        };
        self.fill(role).then_some(role)
    }

    /// Give `role`'s part, as a word of that kind would: false when it is
    /// given already, or a time is given a third time.
    pub fn fill(&mut self, role: Role) -> bool {
        let free = |slot: &mut bool| !std::mem::replace(slot, true);
        match role {
            Role::Time => {
                self.times = self.times.saturating_add(1);
                self.times <= 2
            }
            Role::Easing => free(&mut self.eased),
            Role::Count => free(&mut self.counted),
            Role::Direction => free(&mut self.directed),
            Role::Fill => free(&mut self.filled),
            Role::Play => free(&mut self.stated),
            Role::Name => free(&mut self.named),
        }
    }
}

/// Whether `part` is an easing where an animation's goes: an easing keyword
/// or function in any case ([`word::is_easing`]), or `spring()`, which is
/// refused there by name.
pub fn is_easing(part: &str) -> bool {
    word::is_easing(part)
        || part
            .get(..7)
            .is_some_and(|f| f.eq_ignore_ascii_case("spring("))
}

/// A name as an animation holds it: without its quotes, and `none` in any
/// case as `none`.
fn held_name(part: &str) -> String {
    if part.eq_ignore_ascii_case("none") {
        return "none".into();
    }
    part.trim_matches(|c| c == '"' || c == '\'').to_string()
}

/// Why `part` fits no part of the animation `decl` still has free; `name`
/// is the name it has, if any.
fn left_over(part: &str, decl: &str, name: &str) -> String {
    let again = if is_easing(part) {
        "an easing"
    } else if count(part).is_some() {
        "an iteration count"
    } else if direction(part).is_some() {
        "a direction"
    } else if fill(part).is_some() {
        "a fill mode"
    } else if play_state(part).is_some() {
        "a play state"
    } else if is_name(part) {
        return format!(
            "`{decl}` has a second name, `{part}`, beside `{name}`: an animation has one"
        );
    } else {
        return format!("`{part}` in `{decl}` is no part of an animation: a time, an easing, an iteration count, a direction, a fill mode, a play state or a name, which is an identifier other than `initial`, `inherit`, `unset`, `revert`, `revert-layer` and `default`, or a quoted string");
    };
    format!("`{decl}` gives {again} twice, `{part}` the second time: an animation takes one")
}

fn easing_only(part: &str) -> Result<Easing, ParseError> {
    let refused = |e| {
        match e {
        ParseError::BadEasing(_) => ParseError::BadValue(format!(
            "`{part}` is not an easing: linear, ease, ease-in, ease-out, ease-in-out, step-start, step-end, cubic-bezier(), steps() or linear()"
        )),
        e => e,
    }
    };
    match easing(part).map_err(refused)? {
        TimingFunction::Easing(e) => Ok(e),
        TimingFunction::Spring(_) => Err(ParseError::SpringInAnimation(part.to_string())),
    }
}

fn count(part: &str) -> Option<f64> {
    if part.eq_ignore_ascii_case("infinite") {
        return Some(f64::INFINITY);
    }
    word::number(part).filter(|n| *n >= 0.0)
}

fn direction(part: &str) -> Option<Direction> {
    const DIRECTIONS: [&str; 4] = ["normal", "reverse", "alternate", "alternate-reverse"];
    Some(match word::keyword(part, &DIRECTIONS)? {
        "normal" => Direction::Normal,
        "reverse" => Direction::Reverse,
        "alternate" => Direction::Alternate,
        _ => Direction::AlternateReverse,
    })
}

fn fill(part: &str) -> Option<FillMode> {
    const FILLS: [&str; 4] = ["none", "forwards", "backwards", "both"];
    Some(match word::keyword(part, &FILLS)? {
        "none" => FillMode::None,
        "forwards" => FillMode::Forwards,
        "backwards" => FillMode::Backwards,
        _ => FillMode::Both,
    })
}

/// `running` or `paused`, in any case: whether it is paused.
fn play_state(part: &str) -> Option<bool> {
    word::keyword(part, &["running", "paused"]).map(|state| state == "paused")
}

/// A CSS `<custom-ident>` (or a quoted string): not a CSS-wide keyword nor
/// `default`, which CSS also reserves (CSS Values 4 §4.2).
fn is_name(part: &str) -> bool {
    if part.len() >= 2
        && ((part.starts_with('"') && part.ends_with('"'))
            || (part.starts_with('\'') && part.ends_with('\'')))
    {
        return true;
    }
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let starts = first.is_ascii_alphabetic()
        || first == '_'
        || (first == '-' && part.len() > 1 && !part[1..].starts_with(|c: char| c.is_ascii_digit()));
    starts
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && word::keyword(
            part,
            &[
                "initial",
                "inherit",
                "unset",
                "revert",
                "revert-layer",
                "default",
            ],
        )
        .is_none()
}

fn length(value: &str) -> Option<f64> {
    let n = word::strip_unit(value, "px").unwrap_or(value);
    exact_num::parse_f64(n.trim()).ok()
}

/// One declaration's values: usually one property's, a `light-dark()`
/// colour's dark value beside it (LLP 1062 D9), and `box-shadow`'s two
/// halves, its geometry and its colour.
fn keyframe_values(
    property: Property,
    value: &str,
) -> Result<Vec<(Property, Value, Option<Value>)>, ParseError> {
    let bad = || {
        ParseError::BadValue(format!(
            "`{value}` is not a value `{}` takes",
            property.name()
        ))
    };
    let colour = |text: &str| -> Result<(Value, Option<Value>), ParseError> {
        let text = text.trim();
        if let Some(body) = text
            .strip_prefix("light-dark(")
            .and_then(|b| b.strip_suffix(')'))
        {
            let parts = split_top_level(body, ',');
            let [light, night] = parts.as_slice() else {
                return Err(bad());
            };
            let light = crate::color::parse(light).ok_or_else(bad)?;
            let night = crate::color::parse(night).ok_or_else(bad)?;
            return Ok((light, Some(night)));
        }
        Ok((crate::color::parse(text).ok_or_else(bad)?, None))
    };
    Ok(match property {
        p if p.is_color() => {
            let (light, night) = colour(value)?;
            vec![(p, light, night)]
        }
        Property::FontVariationSettings => variation_axes(value.trim(), bad)?
            .into_iter()
            .map(|(tag, n)| (property, Value::axis(tag, n), None))
            .collect(),
        // `none` is CSS's transparent, zero-length shadow, which a shadow
        // interpolates from and to.
        Property::BoxShadow if value.trim().eq_ignore_ascii_case("none") => vec![
            (Property::BoxShadow, Value::ZERO, None),
            (Property::ShadowColor, Value::ZERO, None),
        ],
        Property::BoxShadow => {
            // `<x> <y> [<blur>] <colour>`, one outer shadow, the colour last.
            let parts: Vec<&str> = split_top_level(value.trim(), ' ')
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            let (color, lengths) = parts.split_last().ok_or_else(bad)?;
            let lengths: Vec<f64> = lengths
                .iter()
                .map(|l| length(l).ok_or_else(bad))
                .collect::<Result<_, _>>()?;
            let (x, y, blur) = match lengths.as_slice() {
                [x, y] => (*x, *y, 0.0),
                [x, y, blur] if *blur >= 0.0 => (*x, *y, *blur),
                _ => return Err(bad()),
            };
            let (light, night) = colour(color)?;
            vec![
                (Property::BoxShadow, Value::four(x, y, blur, 0.0), None),
                (Property::ShadowColor, light, night),
            ]
        }
        p => vec![(p, keyframe_value(p, value)?, None)],
    })
}

fn keyframe_value(property: Property, value: &str) -> Result<Value, ParseError> {
    let bad = || {
        ParseError::BadValue(format!(
            "`{value}` is not a value `{}` takes",
            property.name()
        ))
    };
    Ok(match property {
        // A length or a percentage of the box per axis (chess diary #4):
        // lengths in x and y, percentages in z and w.
        Property::Translate => {
            let axis = |part: &str| match part.strip_suffix('%') {
                Some(n) => exact_num::parse_f64(n.trim())
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(|n| (0.0, n)),
                None => length(part).map(|l| (l, 0.0)),
            };
            let parts: Vec<&str> = value.split_whitespace().collect();
            let ((x, px), (y, py)) = match parts.as_slice() {
                [x] => (axis(x).ok_or_else(bad)?, (0.0, 0.0)),
                [x, y] => (axis(x).ok_or_else(bad)?, axis(y).ok_or_else(bad)?),
                _ => return Err(bad()),
            };
            Value::four(x, y, px, py)
        }
        Property::Rotate => {
            let n = word::strip_unit(value, "deg").unwrap_or(value);
            Value::scalar(exact_num::parse_f64(n.trim()).map_err(|_| bad())?)
        }
        Property::Scale | Property::Opacity => {
            let v = match value.strip_suffix('%') {
                Some(n) => exact_num::parse_f64(n.trim()).map_err(|_| bad())? / 100.0,
                None => exact_num::parse_f64(value).map_err(|_| bad())?,
            };
            Value::scalar(v)
        }
        Property::StrokeDashoffset
        | Property::R
        | Property::Height
        | Property::Cx
        | Property::Cy
        | Property::X
        | Property::Y
        | Property::Rx
        | Property::Ry => Value::scalar(length(value).ok_or_else(bad)?),
        p if p.is_color() => crate::color::parse(value).ok_or_else(bad)?,
        // CSS Fonts 4 §2.2: a number in [1, 1000] or an absolute keyword;
        // `bolder` and `lighter` are relative to a parent no keyframe has.
        Property::FontWeight => Value::scalar(match word::keyword(value, &["normal", "bold"]) {
            Some("normal") => 400.0,
            Some(_) => 700.0,
            None => exact_num::parse_f64(value)
                .ok()
                .filter(|n| (1.0..=1000.0).contains(n))
                .ok_or_else(bad)?,
        }),
        // CSS Fonts 4 §2.4: a non-negative percentage or a keyword.
        Property::FontStretch => {
            let keyword = [
                ("ultra-condensed", 50.0),
                ("extra-condensed", 62.5),
                ("condensed", 75.0),
                ("semi-condensed", 87.5),
                ("normal", 100.0),
                ("semi-expanded", 112.5),
                ("expanded", 125.0),
                ("extra-expanded", 150.0),
                ("ultra-expanded", 200.0),
            ];
            let n = match keyword.iter().find(|(k, _)| value.eq_ignore_ascii_case(k)) {
                Some((_, n)) => *n,
                None => value
                    .strip_suffix('%')
                    .and_then(|n| exact_num::parse_f64(n.trim()).ok())
                    .filter(|n| *n >= 0.0)
                    .ok_or_else(bad)?,
            };
            Value::scalar(n)
        }
        _ => return Err(bad()),
    })
}

/// A `font-variation-settings` list (CSS Fonts 4 §7.2): a quoted
/// four-character tag (U+0020–U+007E) and a number, comma-separated; a tag
/// named twice keeps its last value in its first place. `normal`, which
/// names no axis, is refused here: a keyframe's axes interpolate one by one
/// with the other keyframes', so each keyframe names the same ones.
/// `malformed` is the refusal of an item that is not an axis.
fn variation_axes(
    value: &str,
    malformed: impl Fn() -> ParseError,
) -> Result<Vec<(u32, f64)>, ParseError> {
    if value.eq_ignore_ascii_case("normal") {
        return Err(ParseError::BadValue("`normal` names no axes, and a keyframe's `font-variation-settings` names the axes it animates: `\"wght\" 400`".into()));
    }
    let axis = |item: &str| -> Option<(u32, f64)> {
        let quote = item.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let rest = &item[1..];
        let close = rest.find(quote)?;
        let (tag, number) = (&rest[..close], rest[close + 1..].trim());
        if tag.len() != 4 || !tag.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
            return None;
        }
        let number = exact_num::parse_f64(number)
            .ok()
            .filter(|n| n.is_finite())?;
        Some((u32::from_be_bytes(tag.as_bytes().try_into().ok()?), number))
    };
    let mut out: Vec<(u32, f64)> = Vec::new();
    for item in value.split(',').map(str::trim) {
        let (tag, number) = axis(item).ok_or_else(&malformed)?;
        if let Some(axis) = out.iter_mut().find(|(t, _)| *t == tag) {
            axis.1 = number;
        } else if out.len() < MAX_AXES {
            out.push((tag, number));
        } else {
            return Err(ParseError::BadValue(format!(
                "`{value}` names more than {MAX_AXES} axes: `font-variation-settings` takes at most {MAX_AXES}"
            )));
        }
    }
    Ok(out)
}

/// The most axes one `font-variation-settings` names, as the kernel's row
/// holds them.
pub const MAX_AXES: usize = 8;
