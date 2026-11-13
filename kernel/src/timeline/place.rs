//! Placing scroll and view timelines from layout (LLP 1057.003 D5;
//! Scroll-driven Animations 1 §2, §3).
//!
//! A consumer of a scroll or view timeline is bound in the engine to its
//! scroller's offset on one axis, over the stretch of offsets its
//! `animation-range` covers. That stretch comes from layout: a scroll
//! timeline's runs over the scroller's scroll range, a view timeline's over
//! the offsets where its subject crosses the scrollport. After each layout a
//! host calls [`crate::Kernel::place_timelines`], applies the bindings that
//! changed and watches the scrollers they read
//! ([`crate::Kernel::timeline_scrollers`]), giving the engine each one's
//! offset as it presents it (`Engine::set_scroll_offset`).
//!
//! Boxes are where layout put them, untransformed and at scroll offset zero,
//! as CSS measures a view timeline's subject. A node with no ancestor scroll
//! container scrolls with the page: its root, whose scrollport is the
//! viewport and whose offset is the page's.

use super::lookup::Found;
use super::{reads_x, AnimationRange, Anonymous, RangeName, Scroller, ViewTimelineInset, LINKED};
use crate::arena::NodeArena;
use crate::generated::{Overflow, TimelineAxis};
use crate::id::{NodeFlags, NodeKey, ViewId};
use crate::motion::motion_node;
use exact_motion::NamedTimeline;

/// An engine binding: the timeline and the stretch of it the animations run
/// over (`Engine::set_animation_timeline`).
pub type TimelineBinding = (NamedTimeline, [f64; 2]);
pub(crate) type Binding = TimelineBinding;

/// The linked placement's signature: [`place`].
pub(super) type Place = fn(&mut NodeArena) -> Vec<(u64, Option<Binding>)>;

/// A scroller a placed timeline reads: a host gives the engine its offset
/// whenever it moves (`Engine::set_scroll_offset(node, …)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineScroller {
    /// Its view.
    pub view: ViewId,
    /// Its engine node.
    pub node: u64,
    /// Whether it is a root the page scrolls: its offset is the page's.
    pub page: bool,
}

impl crate::kernel::Kernel {
    /// After a layout, place every consumer of a scroll or view timeline;
    /// the engine bindings that changed since the last placement, to apply
    /// with `Engine::set_animation_timeline`. Nothing while unlinked.
    pub fn place_timelines(&mut self) -> Vec<(u64, Option<TimelineBinding>)> {
        match LINKED.get() {
            Some(grammar) => (grammar.place)(self.arena_mut()),
            None => Vec::new(),
        }
    }

    /// The scrollers the last placement's timelines read.
    pub fn timeline_scrollers(&self) -> &[TimelineScroller] {
        &self.arena().timelines.scrollers
    }
}

/// [`crate::Kernel::place_timelines`], linked.
pub(super) fn place(arena: &mut NodeArena) -> Vec<(u64, Option<Binding>)> {
    let at = &*arena;
    let mut scrollers: Vec<TimelineScroller> = Vec::new();
    let mut placed: Vec<(NodeKey, Binding)> = Vec::new();
    for &(consumer, found) in &at.timelines.resolved {
        let Some(slot) = at.resolve(consumer) else {
            continue;
        };
        let style = at.style(slot);
        let timeline = match found {
            Found::Scroll(key) => at.resolve(key).map(|s| Timeline::Scroll {
                scroller: s,
                axis: at.style(s).rare.scroll_timeline_axis,
            }),
            Found::View(key) => at.resolve(key).map(|s| Timeline::View {
                subject: s,
                axis: at.style(s).rare.view_timeline_axis,
                inset: at.style(s).rare.view_timeline_inset,
            }),
            Found::Anonymous => match style.rare.animation_timeline.anonymous() {
                Some(Anonymous::Scroll { scroller, axis }) => {
                    let s = match scroller {
                        Scroller::Nearest => nearest_scroller(at, slot),
                        Scroller::Root => root_of(at, slot),
                        Scroller::Itself => slot,
                    };
                    Some(Timeline::Scroll { scroller: s, axis })
                }
                Some(Anonymous::View { axis, inset }) => Some(Timeline::View {
                    subject: slot,
                    axis,
                    inset,
                }),
                None => None,
            },
            Found::Drag(_) | Found::Inactive | Found::Missing => continue,
        };
        let binding = timeline
            .and_then(|t| t.place(at, &style.animation_range))
            .map(|(scroller, x, range)| {
                let node = motion_node(at.key(scroller));
                if !scrollers.iter().any(|s| s.node == node) {
                    scrollers.push(TimelineScroller {
                        view: at.local_id(scroller),
                        node,
                        page: !scrolls(at, scroller),
                    });
                }
                (NamedTimeline::Scroll { scroller: node, x }, range)
            })
            .unwrap_or((NamedTimeline::Inactive, [0.0, 1.0]));
        placed.push((consumer, binding));
    }
    let changed = placed
        .iter()
        .filter(|(key, binding)| at.timelines.placed(*key) != Some(*binding))
        .map(|(key, binding)| (motion_node(*key), Some(*binding)))
        .collect();
    arena.timelines.scrollers = scrollers;
    arena.timelines.placed = placed;
    changed
}

/// A scroll or view timeline, before layout places it.
enum Timeline {
    Scroll {
        scroller: u32,
        axis: TimelineAxis,
    },
    View {
        subject: u32,
        axis: TimelineAxis,
        inset: ViewTimelineInset,
    },
}

impl Timeline {
    /// The scroller it reads, whether on `x`, and the offsets where an
    /// animation over `range` is at 0% and 100%; `None` for an inactive
    /// timeline: a scroll timeline on a node that is no scroll container or
    /// has no scroll range on the axis, or a node not laid out.
    fn place(&self, arena: &NodeArena, range: &AnimationRange) -> Option<(u32, bool, [f64; 2])> {
        match *self {
            Self::Scroll { scroller, axis } => {
                let x = reads_x(axis);
                if shown(arena, scroller).is_none()
                    || !(scrolls(arena, scroller) || arena.is_root(scroller))
                {
                    return None;
                }
                let max = scroll_max(arena, scroller, x);
                // 0% and 100% coincide: inactive (§2.1).
                (max > 0.0).then(|| (scroller, x, range.place([0.0, max], |_| None)))
            }
            Self::View {
                subject,
                axis,
                inset,
            } => {
                let x = reads_x(axis);
                shown(arena, subject)?;
                // Unlike a scroll timeline's, a view timeline's progress
                // stands with no scroll range: Chrome 156 shows it (the
                // timelines fixture's `vn`), and Scroll-driven Animations 1
                // §3 makes it inactive only with no box or no scroller.
                let scroller = nearest_scroller(arena, subject);
                let (port_start, port) = scrollport(arena, scroller, x);
                let frame = arena.frame(subject);
                let (start, size) = if x {
                    (frame.x as f64 - port_start, frame.width as f64)
                } else {
                    (frame.y as f64 - port_start, frame.height as f64)
                };
                let inset = insets(arena, scroller, x, port, inset);
                let named = |n: RangeName| n.offsets(start, size, port, inset);
                Some((
                    scroller,
                    x,
                    range.place(named(RangeName::Cover), |n| Some(named(n))),
                ))
            }
        }
    }
}

/// Whether `slot` is a scroll container: `overflow` other than `visible`
/// or `clip` on either axis, as `position: sticky` finds its scroller.
fn scrolls(arena: &NodeArena, slot: u32) -> bool {
    let s = arena.style(slot);
    !matches!(s.overflow_x, Overflow::Visible | Overflow::Clip)
        || !matches!(s.overflow_y, Overflow::Visible | Overflow::Clip)
}

/// `slot`'s nearest ancestor scroll container, else its root.
fn nearest_scroller(arena: &NodeArena, slot: u32) -> u32 {
    let mut at = slot;
    while let Some(parent) = arena.parent(at) {
        if scrolls(arena, parent) {
            return parent;
        }
        at = parent;
    }
    at
}

fn root_of(arena: &NodeArena, slot: u32) -> u32 {
    let mut at = slot;
    while let Some(parent) = arena.parent(at) {
        at = parent;
    }
    at
}

/// `Some` when `slot` and every ancestor were laid out shown.
fn shown(arena: &NodeArena, slot: u32) -> Option<()> {
    let mut at = Some(slot);
    while let Some(s) = at {
        if arena.flags(s).has(NodeFlags::HIDDEN) {
            return None;
        }
        at = arena.parent(s);
    }
    Some(())
}

/// The scrollport's start on the axis, in the layout's space at offset
/// zero, and its length: a box's padding box, or the viewport for a root
/// the page scrolls.
fn scrollport(arena: &NodeArena, scroller: u32, x: bool) -> (f64, f64) {
    let frame = arena.frame(scroller);
    if !scrolls(arena, scroller) {
        let env = arena.env();
        let (origin, length) = if x {
            (frame.x, env.viewport_width)
        } else {
            (frame.y, env.viewport_height)
        };
        return (origin as f64, length as f64);
    }
    let border = arena.inset(scroller).map(|i| i.border).unwrap_or_default();
    // `border` is left, top, right, bottom.
    let (origin, length, start, end) = if x {
        (frame.x, frame.width, border[0], border[2])
    } else {
        (frame.y, frame.height, border[1], border[3])
    };
    (
        (origin + start) as f64,
        (length - start - end).max(0.0) as f64,
    )
}

/// A view timeline's insets in points: an `auto` side is the scroller's
/// `scroll-padding` there, a percentage is of the scrollport.
fn insets(
    arena: &NodeArena,
    scroller: u32,
    x: bool,
    port: f64,
    inset: ViewTimelineInset,
) -> [f64; 2] {
    let s = &arena.style(scroller).rare;
    let padding = if x {
        [&s.scroll_padding_left, &s.scroll_padding_right]
    } else {
        [&s.scroll_padding_top, &s.scroll_padding_bottom]
    };
    let env = &*arena.env_of(scroller);
    let [start, end] = inset.0;
    [(start, padding[0]), (end, padding[1])].map(|(side, padding)| match side {
        Some(offset) => offset.along(0.0, port),
        None => match padding.resolve(env) {
            crate::style::Dimension::Auto => 0.0,
            d => d.at_basis(env, port as f32) as f64,
        },
    })
}

/// How far `scroller` scrolls on the axis: its scrollable overflow less its
/// scrollport; for a root the page scrolls, everything laid out below it
/// down to a box that clips or scrolls, less the viewport.
fn scroll_max(arena: &NodeArena, scroller: u32, x: bool) -> f64 {
    let frame = arena.frame(scroller);
    let axis = |w: f32, h: f32| if x { w } else { h };
    let extent = if scrolls(arena, scroller) {
        // Taffy's block containers do not always count the end padding in
        // their scrollable overflow; CSS's `scrollWidth` does. Floor it with
        // each in-flow child's extent and that padding.
        let (w, h) = arena.content(scroller);
        let pad = arena.inset(scroller).map(|i| i.padding).unwrap_or_default();
        let mut extent = axis(w, h);
        for &child in arena.children(scroller) {
            if arena.blocks.holds(child) || arena.flags(child).has(NodeFlags::HIDDEN) {
                continue;
            }
            let c = arena.frame(child);
            let end = if crate::style::top_layer::layout_position(arena, child)
                == crate::PositionType::Absolute
            {
                0.0
            } else {
                axis(pad[2], pad[3])
            };
            extent = extent.max(axis(c.x - frame.x + c.width, c.y - frame.y + c.height) + end);
        }
        extent
    } else {
        let mut extent = axis(frame.width, frame.height);
        let mut below = arena.children(scroller).to_vec();
        while let Some(child) = below.pop() {
            if arena.blocks.holds(child) || arena.flags(child).has(NodeFlags::HIDDEN) {
                continue;
            }
            let c = arena.frame(child);
            if c.width > 0.0 && c.height > 0.0 {
                extent = extent.max(axis(c.x - frame.x + c.width, c.y - frame.y + c.height));
            }
            if !scrolls(arena, child) {
                below.extend_from_slice(arena.children(child));
            }
        }
        extent
    };
    let (_, port) = scrollport(arena, scroller, x);
    let border = arena.inset(scroller).map(|i| i.border).unwrap_or_default();
    let start_border = if x { border[0] } else { border[1] };
    if scrolls(arena, scroller) {
        extent as f64 - start_border as f64 - port
    } else {
        extent as f64 - port
    }
}
