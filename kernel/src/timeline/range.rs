//! `animation-range` (Scroll-driven Animations 1 §4; CSS Animations 2):
//! where on its timeline an animation runs from 0% to 100%.
//!
//! Each end is `normal`, a `<length-percentage>` along the whole timeline,
//! or a named timeline range and a `<length-percentage>` along it
//! (`exit -100%`, `entry 20px`). A view progress timeline has the six
//! named ranges ([`RangeName`]); a scroll progress timeline and a drag
//! have none, so a name on one stands for the whole timeline, as Chrome
//! reads it. A drag has no length to take a percentage of either: its
//! consumer's range is two lengths, or it is not bound at all.

use std::fmt::Write;

/// A `<length-percentage>` along a timeline or one of its ranges: points
/// from its start, or a percentage of its length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Offset {
    /// Points (`px`, or a bare number).
    Px(f32),
    /// A percentage, as written (`50` for `50%`).
    Percent(f32),
}

impl Offset {
    pub(super) fn parse(token: &str) -> Option<Self> {
        let (n, percent) = match token.strip_suffix('%') {
            Some(n) => (n, true),
            None => (token.strip_suffix("px").unwrap_or(token), false),
        };
        let v: f32 = n.parse().ok()?;
        v.is_finite().then_some(if percent {
            Self::Percent(v)
        } else {
            Self::Px(v)
        })
    }

    /// The point `self` names on a stretch from `start` to `end`.
    pub fn along(self, start: f64, end: f64) -> f64 {
        match self {
            Self::Px(v) => start + v as f64,
            Self::Percent(p) => start + (end - start) * p as f64 / 100.0,
        }
    }

    pub(super) fn write(self, out: &mut String) {
        let _ = match self {
            Self::Px(v) => write!(out, "{}px", exact_num::Shortest32(v)),
            Self::Percent(p) => write!(out, "{}%", exact_num::Shortest32(p)),
        };
    }

    pub(super) fn is_finite(self) -> bool {
        match self {
            Self::Px(v) | Self::Percent(v) => v.is_finite(),
        }
    }
}

/// A view progress timeline's named ranges (Scroll-driven Animations 1
/// §3.1), each a stretch of scroll offsets where the subject's border box
/// stands so against the scrollport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeName {
    /// From the subject's start edge at the scrollport's end edge to its end
    /// edge at the scrollport's start edge: the whole timeline.
    Cover,
    /// From fully inside the scrollport to leaving it; or, for a subject
    /// larger than the scrollport, from covering it to no longer covering it.
    Contain,
    /// From 0% of `cover` to 0% of `contain`: entering.
    Entry,
    /// From 100% of `contain` to 100% of `cover`: leaving.
    Exit,
    /// The subject's start edge, then its end edge, at the scrollport's end edge.
    EntryCrossing,
    /// The subject's start edge, then its end edge, at the scrollport's start edge.
    ExitCrossing,
}

impl RangeName {
    const NAMES: [(&'static str, Self); 6] = [
        ("cover", Self::Cover),
        ("contain", Self::Contain),
        ("entry", Self::Entry),
        ("exit", Self::Exit),
        ("entry-crossing", Self::EntryCrossing),
        ("exit-crossing", Self::ExitCrossing),
    ];

    fn parse(token: &str) -> Option<Self> {
        (Self::NAMES.iter())
            .find(|(n, _)| token.eq_ignore_ascii_case(n))
            .map(|&(_, r)| r)
    }

    /// The name as CSS writes it.
    pub fn name(self) -> &'static str {
        (Self::NAMES.iter())
            .find(|(_, r)| *r == self)
            .map_or("", |(n, _)| n)
    }

    /// The scroll offsets where this range is at 0% and 100%, for a subject
    /// `size` long whose start edge is `start` into its scroller's content,
    /// in a scrollport `port` long whose edges are inset by `inset` (start,
    /// end).
    pub fn offsets(self, start: f64, size: f64, port: f64, inset: [f64; 2]) -> [f64; 2] {
        // The offsets where each subject edge meets each scrollport edge.
        let start_at_end = start - port + inset[1];
        let end_at_end = start + size - port + inset[1];
        let start_at_start = start - inset[0];
        let end_at_start = start + size - inset[0];
        let contain = [
            end_at_end.min(start_at_start),
            end_at_end.max(start_at_start),
        ];
        match self {
            Self::Cover => [start_at_end, end_at_start],
            Self::Contain => contain,
            Self::Entry => [start_at_end, contain[0]],
            Self::Exit => [contain[1], end_at_start],
            Self::EntryCrossing => [start_at_end, end_at_end],
            Self::ExitCrossing => [start_at_start, end_at_start],
        }
    }
}

/// One end of `animation-range`: a point along the whole timeline (no
/// name), or along one of its named ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeEdge {
    /// The named range, if one is named.
    pub name: Option<RangeName>,
    /// The point along it.
    pub offset: Offset,
}

impl RangeEdge {
    /// `normal` at the start: the timeline's own start.
    pub const START: Self = Self {
        name: None,
        offset: Offset::Percent(0.0),
    };
    /// `normal` at the end: the timeline's own end.
    pub const END: Self = Self {
        name: None,
        offset: Offset::Percent(100.0),
    };
}

/// `animation-range`: where on the timeline the animations run from 0% to
/// 100%; `None` is `normal` at both ends.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnimationRange(pub Option<[RangeEdge; 2]>);

impl AnimationRange {
    /// `normal`, or one or two ends, each `normal`, a `<length-percentage>`
    /// or a range name with an optional `<length-percentage>` (0% at the
    /// start, 100% at the end); a lone named start ends at its range's 100%.
    /// Two equal ends are refused. `None` also while unlinked.
    pub fn parse(css: &str) -> Option<Self> {
        (super::LINKED.get()?.range)(css)
    }

    pub(super) fn parse_text(css: &str) -> Option<Self> {
        let tokens: Vec<&str> = css.split_ascii_whitespace().collect();
        let mut edges: Vec<RangeEdge> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let token = tokens[i];
            i += 1;
            let at_start = edges.is_empty();
            let whole = if at_start {
                RangeEdge::START
            } else {
                RangeEdge::END
            };
            let edge = if token.eq_ignore_ascii_case("normal") {
                whole
            } else if let Some(name) = RangeName::parse(token) {
                let offset = match tokens.get(i).and_then(|t| Offset::parse(t)) {
                    Some(offset) => {
                        i += 1;
                        offset
                    }
                    None => whole.offset,
                };
                RangeEdge {
                    name: Some(name),
                    offset,
                }
            } else {
                RangeEdge {
                    name: None,
                    offset: Offset::parse(token)?,
                }
            };
            edges.push(edge);
        }
        let ends = match edges[..] {
            [start] if start == RangeEdge::START => return Some(Self(None)),
            // A lone start ends at its range's 100%, or at `normal`.
            [start] => [
                start,
                RangeEdge {
                    name: start.name,
                    offset: Offset::Percent(100.0),
                },
            ],
            [start, end] if start == RangeEdge::START && end == RangeEdge::END => {
                return Some(Self(None))
            }
            [start, end] => [start, end],
            _ => return None,
        };
        (ends[0] != ends[1]).then_some(Self(Some(ends)))
    }

    /// The declaration's value.
    pub fn css(&self) -> String {
        let Some(ends) = self.0 else {
            return "normal".into();
        };
        let mut out = String::new();
        for (i, (edge, whole)) in ends
            .iter()
            .zip([RangeEdge::START, RangeEdge::END])
            .enumerate()
        {
            if i > 0 {
                out.push(' ');
            }
            match edge.name {
                None if *edge == whole => out.push_str("normal"),
                None => edge.offset.write(&mut out),
                Some(name) => {
                    out.push_str(name.name());
                    out.push(' ');
                    edge.offset.write(&mut out);
                }
            }
        }
        out
    }

    /// Whether every number in it is finite.
    pub fn is_finite(&self) -> bool {
        self.0
            .is_none_or(|ends| ends.iter().all(|e| e.offset.is_finite()))
    }

    /// Two lengths on the whole timeline, as a drag's consumer takes them:
    /// the positions where its animations are at 0% and 100%. `None` for
    /// anything a drag has no length for (`normal`, a percentage, a name).
    pub fn lengths(&self) -> Option<[f32; 2]> {
        match self.0? {
            [RangeEdge {
                name: None,
                offset: Offset::Px(a),
            }, RangeEdge {
                name: None,
                offset: Offset::Px(b),
            }] => Some([a, b]),
            _ => None,
        }
    }

    /// The scroll offsets where the animations are at 0% and 100% on a
    /// timeline whose whole runs from `whole[0]` to `whole[1]`, with
    /// `named` giving a named range's offsets (`None` for a timeline with
    /// none, where a name stands for the whole).
    pub fn place(
        &self,
        whole: [f64; 2],
        named: impl Fn(RangeName) -> Option<[f64; 2]>,
    ) -> [f64; 2] {
        let ends = self.0.unwrap_or([RangeEdge::START, RangeEdge::END]);
        ends.map(|edge| {
            let [a, b] = edge.name.and_then(&named).unwrap_or(whole);
            edge.offset.along(a, b)
        })
    }
}
