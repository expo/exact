//! Scroll and view progress timelines' own grammar (Scroll-driven
//! Animations 1 §2, §3): the names a scroller or a subject declares, a view
//! timeline's insets, and the anonymous `scroll()` and `view()` an
//! `animation-timeline` may name instead.
//!
//! Exact takes one timeline per node and kind: a comma-separated list of
//! names or insets is refused, not read in part.

use super::range::Offset;
use crate::generated::TimelineAxis;

/// `scroll-timeline-name` or `view-timeline-name`: `none`, or one
/// `<dashed-ident>`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineName(pub Option<String>);

impl TimelineName {
    /// `none` or a `<dashed-ident>`; `None` also while unlinked.
    pub fn parse(css: &str) -> Option<Self> {
        (super::LINKED.get()?.name)(css)
    }

    pub(super) fn parse_text(css: &str) -> Option<Self> {
        let t = css.trim();
        if t.eq_ignore_ascii_case("none") {
            return Some(Self(None));
        }
        super::dashed(t).map(|n| Self(Some(n)))
    }

    /// The declaration's value.
    pub fn css(&self) -> String {
        self.0.clone().unwrap_or_else(|| "none".into())
    }
}

/// `view-timeline-inset`: how far the scrollport's start and end edges move
/// in for a view timeline; `None` is `auto`, the scroller's
/// `scroll-padding` on that side.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewTimelineInset(pub [Option<Offset>; 2]);

impl ViewTimelineInset {
    /// One or two of `auto` and a length or percentage; `None` also while
    /// unlinked.
    pub fn parse(css: &str) -> Option<Self> {
        (super::LINKED.get()?.inset)(css)
    }

    pub(super) fn parse_text(css: &str) -> Option<Self> {
        let sides: Vec<Option<Offset>> = (css.split_ascii_whitespace())
            .map(inset_side)
            .collect::<Option<_>>()?;
        match sides[..] {
            [both] => Some(Self([both, both])),
            [start, end] => Some(Self([start, end])),
            _ => None,
        }
    }

    /// The declaration's value: one value when both sides are the same.
    pub fn css(&self) -> String {
        let side = |s: Option<Offset>| match s {
            None => "auto".to_string(),
            Some(offset) => {
                let mut out = String::new();
                offset.write(&mut out);
                out
            }
        };
        let [start, end] = self.0;
        if start == end {
            side(start)
        } else {
            format!("{} {}", side(start), side(end))
        }
    }

    /// Whether every number in it is finite.
    pub fn is_finite(&self) -> bool {
        self.0.iter().flatten().all(|o| o.is_finite())
    }
}

/// `auto`, or a `<length-percentage>`; the outer `None` is neither.
fn inset_side(token: &str) -> Option<Option<Offset>> {
    if token.eq_ignore_ascii_case("auto") {
        Some(None)
    } else {
        Offset::parse(token).map(Some)
    }
}

/// The scroller `scroll()` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scroller {
    /// The nearest ancestor scroll container (the page's, when none).
    #[default]
    Nearest,
    /// The page's.
    Root,
    /// The consumer itself.
    Itself,
}

/// An anonymous timeline: `scroll()` or `view()` in `animation-timeline`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anonymous {
    /// `scroll(<scroller> || <axis>)`: a scroll progress timeline of the
    /// scroller named relative to the consumer.
    Scroll {
        /// Which scroller.
        scroller: Scroller,
        /// The axis it reads.
        axis: TimelineAxis,
    },
    /// `view(<axis> || <inset>)`: a view progress timeline whose subject is
    /// the consumer.
    View {
        /// The axis it reads.
        axis: TimelineAxis,
        /// Its insets.
        inset: ViewTimelineInset,
    },
}

impl Anonymous {
    /// `scroll(…)` or `view(…)`, either argument order, case-insensitive.
    pub(super) fn parse(css: &str) -> Option<Self> {
        let t = css.trim();
        let open = t.find('(')?;
        let args = t[open + 1..].strip_suffix(')')?;
        let function = t[..open].trim_end();
        let words: Vec<&str> = args.split_ascii_whitespace().collect();
        let axis_of = |w: &str| TimelineAxis::from_name(&w.to_ascii_lowercase());
        if function.eq_ignore_ascii_case("scroll") {
            let (mut scroller, mut axis) = (None, None);
            for w in &words {
                let named = match w.to_ascii_lowercase().as_str() {
                    "nearest" => Some(Scroller::Nearest),
                    "root" => Some(Scroller::Root),
                    "self" => Some(Scroller::Itself),
                    _ => None,
                };
                match (named, axis_of(w)) {
                    (Some(s), _) if scroller.is_none() => scroller = Some(s),
                    (None, Some(a)) if axis.is_none() => axis = Some(a),
                    _ => return None,
                }
            }
            return Some(Self::Scroll {
                scroller: scroller.unwrap_or_default(),
                axis: axis.unwrap_or_default(),
            });
        }
        if !function.eq_ignore_ascii_case("view") {
            return None;
        }
        // `<axis> || <inset>`: the axis on either side of one or two insets.
        let (mut axis, mut sides) = (None, Vec::new());
        for (i, w) in words.iter().enumerate() {
            match axis_of(w) {
                Some(a) if axis.is_none() && (i == 0 || i == words.len() - 1) => axis = Some(a),
                Some(_) => return None,
                None => sides.push(inset_side(w)?),
            }
        }
        let inset = match sides[..] {
            [] => ViewTimelineInset::default(),
            [both] => ViewTimelineInset([both, both]),
            [start, end] => ViewTimelineInset([start, end]),
            _ => return None,
        };
        Some(Self::View {
            axis: axis.unwrap_or_default(),
            inset,
        })
    }

    /// The function as CSS serializes it: its arguments in order, the
    /// defaults left out.
    pub(super) fn css(&self) -> String {
        let mut words: Vec<String> = Vec::new();
        match self {
            Self::Scroll { scroller, axis } => {
                match scroller {
                    Scroller::Nearest => {}
                    Scroller::Root => words.push("root".into()),
                    Scroller::Itself => words.push("self".into()),
                }
                if *axis != TimelineAxis::Block {
                    words.push(axis.name().into());
                }
                format!("scroll({})", words.join(" "))
            }
            Self::View { axis, inset } => {
                if *axis != TimelineAxis::Block {
                    words.push(axis.name().into());
                }
                if *inset != ViewTimelineInset::default() {
                    words.push(inset.css());
                }
                format!("view({})", words.join(" "))
            }
        }
    }
}

/// Whether `axis` reads the horizontal offset. Exact lays out in a
/// horizontal writing mode, where the inline axis is `x`.
pub fn reads_x(axis: TimelineAxis) -> bool {
    matches!(axis, TimelineAxis::Inline | TimelineAxis::X)
}
