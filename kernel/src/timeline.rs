//! Animation timelines: a scroller's offset, a node's progress through its
//! scroller, or a drag (LLP 1057.003 D1, D4, D5).
//!
//! CSS scroll-driven animations (CSS Animations 2; Scroll-driven
//! Animations 1) bind a node's `animation`s to a timeline
//! (`animation-timeline`) over a stretch of it (`animation-range`) instead
//! of the clock:
//!
//! - `scroll-timeline-name` and `-axis` on a scroll container: a scroll
//!   progress timeline, its scroll offset on the axis from the start of its
//!   scroll range to the end.
//! - `view-timeline-name`, `-axis` and `-inset` on any node: a view progress
//!   timeline, the node's border box crossing the scrollport of its nearest
//!   ancestor scroll container.
//! - `animation-timeline: auto | <dashed-ident> | scroll() | view()` on a
//!   consumer: a named timeline, or an anonymous one the consumer's own
//!   place gives ([`Anonymous`]). Its fifth value, `-exact-clock(<ident>)`,
//!   keeps the animations on the clock and only syncs their starts (LLP
//!   1055.002); the lookup never sees it.
//! - `animation-range` on the consumer: where on the timeline its
//!   animations are at 0% and 100% ([`AnimationRange`]).
//! - `timeline-scope: none | all | <dashed-ident>#`: names declared below a
//!   node, in scope for the node's subtree.
//!
//! A drag has no CSS timeline. `-exact-drag-timeline: none | <dashed-ident>
//! [x | y]?` on the node whose held translate a drag moves is the same shape
//! with the drag as the source, a deviation LLP 1001 declares: the
//! timeline's position is that translate on the axis (`y` if unsaid), as
//! presented, so a release's spring moves it too. Its consumers take two
//! lengths in `animation-range`, since a drag has no length to take a
//! percentage of and no named ranges.
//!
//! The engine evaluates a consumer in the frame its source moves (D2); no
//! app code runs per frame. Names resolve as CSS resolves them, in the
//! kernel ([`lookup`], D4), so the engine hears a node, never a name. A
//! scroll or view timeline's stretch of scroll offsets comes from layout:
//! after each layout a host places them ([`place`]) and gives the engine
//! each scroller's offset as it presents it, so a consumer moves in the
//! frame its scroller does.
//!
//! The grammar and the lookup are linked by use (LLP 1047 D2): until a host
//! calls [`link`], a text value for any of the rows is refused as a bad
//! value and no name resolves. The compiler and the native hosts link it at
//! start; a web artifact links it when its plan sets one of the rows, which
//! a plan that sets none never reaches (D6).

mod lookup;
mod place;
mod range;
mod scroll;
/// What a name resolves to, as the engine takes it ([`crate::Kernel::timeline_of`]).
pub use exact_motion::NamedTimeline;
pub use lookup::Found;
pub(crate) use lookup::{refresh, rows, Registry};
pub use place::{TimelineBinding, TimelineScroller};
pub use range::{AnimationRange, Offset, RangeEdge, RangeName};
pub use scroll::{reads_x, Anonymous, Scroller, TimelineName, ViewTimelineInset};

/// The rows' grammar, the name lookup and the placement, once linked
/// ([`link`]).
static LINKED: std::sync::OnceLock<Grammar> = std::sync::OnceLock::new();

struct Grammar {
    drag: fn(&str) -> Option<DragTimeline>,
    timeline: fn(&str) -> Option<AnimationTimeline>,
    range: fn(&str) -> Option<AnimationRange>,
    scope: fn(&str) -> Option<TimelineScope>,
    name: fn(&str) -> Option<TimelineName>,
    inset: fn(&str) -> Option<ViewTimelineInset>,
    resolve: lookup::Resolve,
    place: place::Place,
}

/// Link the rows' grammar, the name lookup and the placement into this
/// artifact.
pub fn link() {
    let _ = LINKED.set(Grammar {
        drag: DragTimeline::parse_text,
        timeline: AnimationTimeline::parse_text,
        range: AnimationRange::parse_text,
        scope: TimelineScope::parse_text,
        name: TimelineName::parse_text,
        inset: ViewTimelineInset::parse_text,
        resolve: lookup::resolve,
        place: place::place,
    });
}

/// The axis of a held translate a drag timeline reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Axis {
    /// The horizontal translate.
    X,
    /// The vertical translate.
    #[default]
    Y,
}

/// `-exact-drag-timeline`: the timeline a node's held translate drives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DragTimeline {
    /// The timeline's name, a `<dashed-ident>`; `None` is `none`.
    pub name: Option<String>,
    /// The axis it reads.
    pub axis: Axis,
}

/// The clock timeline function, with its kind (LLP 1081 D8): Exact's, as
/// CSS has no clock timeline.
pub const CLOCK_FUNCTION: (&str, &str) = ("-exact-clock(", "exact LLP 1055.002");

/// `-exact-clock(<ident>)`'s ident: a Contract name, `[A-Za-z_][A-Za-z0-9_-]*`.
fn clock_name(token: &str) -> Option<&str> {
    let name = token
        .strip_prefix(CLOCK_FUNCTION.0)?
        .strip_suffix(')')?
        .trim();
    let mut chars = name.chars();
    (chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then_some(name)
}

fn dashed(token: &str) -> Option<String> {
    let rest = token.strip_prefix("--")?;
    (!rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    .then(|| token.to_string())
}

impl DragTimeline {
    /// `none`, or a `<dashed-ident>` and an optional axis; `None` also
    /// while the grammar is unlinked ([`link`]).
    pub fn parse(css: &str) -> Option<Self> {
        (LINKED.get()?.drag)(css)
    }

    fn parse_text(css: &str) -> Option<Self> {
        let mut tokens = css.split_ascii_whitespace();
        let first = tokens.next()?;
        if first.eq_ignore_ascii_case("none") {
            return tokens.next().is_none().then(Self::default);
        }
        let name = dashed(first)?;
        let axis = match tokens.next() {
            None => Axis::Y,
            Some(a) if a.eq_ignore_ascii_case("y") || a.eq_ignore_ascii_case("block") => Axis::Y,
            Some(a) if a.eq_ignore_ascii_case("x") || a.eq_ignore_ascii_case("inline") => Axis::X,
            Some(_) => return None,
        };
        tokens.next().is_none().then_some(Self {
            name: Some(name),
            axis,
        })
    }

    /// The declaration's value.
    pub fn css(&self) -> String {
        match &self.name {
            None => "none".into(),
            Some(n) => format!("{n} {}", if self.axis == Axis::X { "x" } else { "y" }),
        }
    }
}

/// `animation-timeline`: `auto` (the clock), a named timeline, or a clock
/// timeline, `-exact-clock(<ident>)`: the clock, from a start every animation on
/// the same one shares (LLP 1055.002 D2, a deviation LLP 1001 declares).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimationTimeline(pub Option<String>);

impl AnimationTimeline {
    /// `auto`, a `<dashed-ident>` or `-exact-clock(<ident>)`; `None` also while
    /// unlinked.
    pub fn parse(css: &str) -> Option<Self> {
        (LINKED.get()?.timeline)(css)
    }

    fn parse_text(css: &str) -> Option<Self> {
        let t = css.trim();
        if t.eq_ignore_ascii_case("auto") {
            return Some(Self(None));
        }
        if let Some(name) = clock_name(t) {
            return Some(Self(Some(format!("-exact-clock({name})"))));
        }
        if let Some(anonymous) = Anonymous::parse(t) {
            return Some(Self(Some(anonymous.css())));
        }
        dashed(t).map(|n| Self(Some(n)))
    }

    /// The named timeline's `<dashed-ident>`, which the lookup resolves.
    pub fn name(&self) -> Option<&str> {
        self.0.as_deref().filter(|t| t.starts_with("--"))
    }

    /// The anonymous timeline, `scroll()` or `view()`.
    pub fn anonymous(&self) -> Option<Anonymous> {
        self.0
            .as_deref()
            .filter(|t| t.ends_with(')') && !t.starts_with('-'))
            .and_then(Anonymous::parse)
    }

    /// The clock timeline's name.
    pub fn clock(&self) -> Option<&str> {
        self.0.as_deref().and_then(clock_name)
    }

    /// The declaration's value.
    pub fn css(&self) -> String {
        self.0.clone().unwrap_or_else(|| "auto".into())
    }
}
/// `timeline-scope` (Scroll-driven Animations 1 §4.2): the timeline names a
/// node declares in scope for its subtree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TimelineScope {
    /// `none`: no change in scope.
    #[default]
    None,
    /// `all`: every name a descendant declares.
    All,
    /// A list of `<dashed-ident>`s, as its canonical text: `--a, --b`. One
    /// string, not a list of them, is less code in every artifact that
    /// carries the row and sets none (LLP 1047 D9).
    Names(String),
}

impl TimelineScope {
    /// `none`, `all`, or a comma-separated list of `<dashed-ident>`s;
    /// `None` also while unlinked.
    pub fn parse(css: &str) -> Option<Self> {
        (LINKED.get()?.scope)(css)
    }

    fn parse_text(css: &str) -> Option<Self> {
        let t = css.trim();
        if t.eq_ignore_ascii_case("none") {
            return Some(Self::None);
        }
        if t.eq_ignore_ascii_case("all") {
            return Some(Self::All);
        }
        let mut names = String::new();
        for name in t.split(',') {
            if !names.is_empty() {
                names.push_str(", ");
            }
            names.push_str(&dashed(name.trim())?);
        }
        Some(Self::Names(names))
    }

    /// The declaration's value.
    pub fn css(&self) -> String {
        match self {
            Self::None => "none".into(),
            Self::All => "all".into(),
            Self::Names(names) => names.clone(),
        }
    }

    /// Whether it scopes `name`: `all` scopes every name.
    fn scopes(&self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Names(names) => names.split(", ").any(|n| n == name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_round_trip() {
        link();
        let d = DragTimeline::parse("--dismiss y").unwrap();
        assert_eq!(d.name.as_deref(), Some("--dismiss"));
        assert_eq!(DragTimeline::parse(&d.css()), Some(d));
        assert_eq!(DragTimeline::parse("--pan x").unwrap().axis, Axis::X);
        assert_eq!(DragTimeline::parse("none"), Some(DragTimeline::default()));
        assert_eq!(DragTimeline::parse("dismiss"), None);
        assert_eq!(
            AnimationTimeline::parse("--dismiss").unwrap().css(),
            "--dismiss"
        );
        assert_eq!(
            AnimationTimeline::parse("auto"),
            Some(AnimationTimeline(None))
        );
        let c = AnimationTimeline::parse(" -exact-clock( Pending ) ").unwrap();
        assert_eq!(
            (c.css().as_str(), c.clock(), c.name()),
            ("-exact-clock(Pending)", Some("Pending"), None)
        );
        assert_eq!(AnimationTimeline::parse("--a").unwrap().clock(), None);
        // Contract names may hyphenate.
        let c = AnimationTimeline::parse("-exact-clock(Pending-Work)").unwrap();
        assert_eq!(c.clock(), Some("Pending-Work"));
        for bad in [
            "-exact-clock()",
            "-exact-clock(--a)",
            "-exact-clock(1a)",
            "-exact-clock(a b)",
            "clock",
            "clock(Pending)",
        ] {
            assert_eq!(AnimationTimeline::parse(bad), None, "{bad:?}");
        }
        let r = AnimationRange::parse("0px 300px").unwrap();
        assert_eq!(r.css(), "0px 300px");
        assert_eq!(r.lengths(), Some([0.0, 300.0]));
        assert_eq!(AnimationRange::parse("10 10"), None);
        let s = TimelineScope::parse(" --a,--b ").unwrap();
        assert_eq!(s.css(), "--a, --b");
        assert_eq!(TimelineScope::parse(&s.css()), Some(s));
        assert_eq!(TimelineScope::parse("ALL"), Some(TimelineScope::All));
        assert_eq!(TimelineScope::parse("none"), Some(TimelineScope::None));
        for bad in ["", "--a --b", "--a,", "a", "--a, all", "none, --a"] {
            assert_eq!(TimelineScope::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn scroll_and_view_rows_read_css_and_write_it_back() {
        use crate::generated::TimelineAxis;
        link();
        assert_eq!(TimelineName::parse("--cols").unwrap().css(), "--cols");
        assert_eq!(TimelineName::parse("NONE"), Some(TimelineName(None)));
        for bad in ["cols", "--a, --b", "--a --b", ""] {
            assert_eq!(TimelineName::parse(bad), None, "{bad:?}");
        }
        for (text, css) in [
            ("auto", "auto"),
            ("10px", "10px"),
            ("10 20%", "10px 20%"),
            ("auto 4px", "auto 4px"),
            ("5% 5%", "5%"),
        ] {
            assert_eq!(ViewTimelineInset::parse(text).unwrap().css(), css, "{text}");
        }
        for bad in ["", "1px 2px 3px", "1em", "auto, auto"] {
            assert_eq!(ViewTimelineInset::parse(bad), None, "{bad:?}");
        }
        // Anonymous timelines, either argument order, defaults left out.
        for (text, css) in [
            ("scroll()", "scroll()"),
            ("SCROLL(x root)", "scroll(root x)"),
            ("scroll(nearest block)", "scroll()"),
            ("scroll(self inline)", "scroll(self inline)"),
            ("view()", "view()"),
            ("view(10px x)", "view(x 10px)"),
            ("view(y auto 20%)", "view(y auto 20%)"),
        ] {
            let t = AnimationTimeline::parse(text).unwrap();
            assert_eq!(t.css(), css, "{text}");
            assert!(t.anonymous().is_some() && t.name().is_none() && t.clock().is_none());
        }
        assert_eq!(
            AnimationTimeline::parse("scroll(root x)")
                .unwrap()
                .anonymous(),
            Some(Anonymous::Scroll {
                scroller: Scroller::Root,
                axis: TimelineAxis::X
            })
        );
        for bad in [
            "scroll(root root)",
            "scroll(x y)",
            "scroll(far)",
            "view(10px x 20px)",
            "view(1px 2px 3px)",
            "view(x y)",
            "scroll(",
        ] {
            assert_eq!(AnimationTimeline::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_range_takes_names_percentages_and_normal_at_either_end() {
        link();
        for (text, css) in [
            ("normal", "normal"),
            ("normal normal", "normal"),
            ("0% 100%", "normal"),
            ("0px 300px", "0px 300px"),
            ("20%", "20% normal"),
            ("normal 300px", "normal 300px"),
            ("exit", "exit 0% exit 100%"),
            ("exit -100% exit 0%", "exit -100% exit 0%"),
            ("entry 10% cover", "entry 10% cover 100%"),
            (
                "ENTRY-CROSSING 20px exit-crossing",
                "entry-crossing 20px exit-crossing 100%",
            ),
            ("contain 50", "contain 50px contain 100%"),
        ] {
            let r = AnimationRange::parse(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(r.css(), css, "{text}");
            assert_eq!(AnimationRange::parse(&r.css()), Some(r), "{text}");
        }
        for bad in [
            "",
            "10px 10px",
            "exit 0% exit 0%",
            "a b",
            "1px 2px 3px",
            "entry exit cover",
            "1em",
        ] {
            assert_eq!(AnimationRange::parse(bad), None, "{bad:?}");
        }
        assert_eq!(AnimationRange::parse("20% 80%").unwrap().lengths(), None);
        // Placed on a timeline from 100 to 300 whose `exit` runs 250 to 300.
        let named = |n: RangeName| (n == RangeName::Exit).then_some([250.0, 300.0]);
        let place = |text: &str| {
            AnimationRange::parse(text)
                .unwrap()
                .place([100.0, 300.0], named)
        };
        assert_eq!(place("normal"), [100.0, 300.0]);
        assert_eq!(place("10% 50px"), [120.0, 150.0]);
        assert_eq!(place("exit"), [250.0, 300.0]);
        assert_eq!(place("exit -100% exit 0%"), [200.0, 250.0]);
        // A name the timeline lacks stands for the whole.
        assert_eq!(place("contain 50%"), [200.0, 300.0]);
    }

    #[test]
    fn the_named_ranges_are_csss() {
        // A 100-long subject 400 into its scroller's content, a 300-long
        // scrollport, insets 10 at the start and 20 at the end.
        let at = |n: RangeName| n.offsets(400.0, 100.0, 300.0, [10.0, 20.0]);
        assert_eq!(at(RangeName::Cover), [120.0, 490.0]);
        assert_eq!(at(RangeName::Contain), [220.0, 390.0]);
        assert_eq!(at(RangeName::Entry), [120.0, 220.0]);
        assert_eq!(at(RangeName::Exit), [390.0, 490.0]);
        assert_eq!(at(RangeName::EntryCrossing), [120.0, 220.0]);
        assert_eq!(at(RangeName::ExitCrossing), [390.0, 490.0]);
        // Larger than the scrollport: `contain` is while it covers it.
        let big = |n: RangeName| n.offsets(400.0, 500.0, 300.0, [0.0, 0.0]);
        assert_eq!(big(RangeName::Contain), [400.0, 600.0]);
        assert_eq!(big(RangeName::Entry), [100.0, 400.0]);
        assert_eq!(big(RangeName::EntryCrossing), [100.0, 600.0]);
        assert_eq!(big(RangeName::Exit), [600.0, 900.0]);
    }
}
