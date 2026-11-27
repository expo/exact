//! CSS Scroll Snap 1 on the Linux family (LLP 1008 §9, "Scroll snap", the
//! definition every native host shares): along one axis, the offsets a
//! scroller's snap areas rest at, and which of them a scroll that ends rests
//! at. Android's reader aims its fling with them ([`Presenter::reader_snaps`],
//! `Scrolling.kt`'s `snapRest`); a desktop wheel snaps here (`wheel.rs`).
//! Apple's is `host/apple/Sources/ExactKit/ScrollSnap.swift`.
use super::reveal::Geometry;
use super::*;
use exact_kernel::id::IdSet;
use exact_kernel::style::snap::SnapAlign;
use exact_kernel::Display;
use std::collections::BTreeSet;

type Stamp = (u64, u64, u64);

#[derive(Default)]
pub(super) struct Layouts {
    declared: Option<bool>,
    scanned: Option<Stamp>,
    checked: Option<(Stamp, (f32, f32), f32)>,
    offered: BTreeMap<(ViewId, bool), AxisLayout>,
    touch_scale: f32,
    #[cfg(test)]
    pub(super) probe_paints: usize,
    axes: BTreeMap<(ViewId, bool), AxisLayout>,
}

struct AxisLayout {
    stamp: Stamp,
    snap: Snap,
    // A range clamp preserves the area the old offset belonged to.
    clamped: Option<(f32, f32)>,
    // The chosen area's offset and the landing the platform can reach.
    release: Option<(f32, f32)>,
    absent: Option<Stamp>,
    preferred: Option<ViewId>,
}

impl AxisLayout {
    fn new(stamp: Stamp, snap: Snap, preferred: Option<ViewId>) -> Self {
        Self {
            stamp,
            snap,
            preferred,
            clamped: None,
            release: None,
            absent: None,
        }
    }

    fn before_clamp(&self, current: f32) -> f32 {
        self.clamped
            .filter(|(to, _)| *to == current)
            .map_or(current, |(_, from)| from)
    }

    fn rested(&self, current: f32) -> Option<(ViewId, f32)> {
        self.area_at(self.before_clamp(current))
    }

    fn area_at(&self, at: f32) -> Option<(ViewId, f32)> {
        self.snap
            .areas
            .iter()
            .filter(|a| a.id != 0 && at >= a.from - 0.5 && at <= a.to + 0.5)
            .min_by_key(|a| Some(a.id) != self.preferred)
            .map(|a| (a.id, at))
    }
}

impl Layouts {
    pub(super) fn clamped(&mut self, id: ViewId, from: (f32, f32), to: (f32, f32)) {
        for (vertical, before, after) in [(false, from.0, to.0), (true, from.1, to.1)] {
            if before != after {
                if let Some(axis) = self.axes.get_mut(&(id, vertical)) {
                    axis.clamped = Some((after, axis.before_clamp(before)));
                }
            }
        }
    }
}

/// The offsets one snap area rests at, `from` through `to`, and whether a
/// snap may not pass over them (`scroll-snap-stop: always`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapArea {
    /// The least offset it rests at.
    pub from: f32,
    /// The greatest: `from` but for an area longer than the snapport.
    pub to: f32,
    /// `scroll-snap-stop: always`.
    pub stop: bool,
    /// Its node; 0 for a list's row that is not mounted.
    pub id: ViewId,
}

/// A scroller's snap along one axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    /// Every rest is a snap position (`mandatory`), not only one near one.
    pub mandatory: bool,
    /// The snapport's length.
    pub port: f32,
    /// Its areas' rests, as offsets.
    pub areas: Vec<SnapArea>,
}

/// Quiet window (ms) after the last fine wheel tick before a snapping scroller
/// settles, matching macOS's `ScrollEnd.quiet` and Chromium's wheel-end timer.
const WHEEL_SNAP_QUIET_MS: f64 = 100.0;

/// Reverse travel (pt) from the farthest offset reached during a fine wheel run
/// that cancels its forward flick direction.
const WHEEL_PULLBACK_SLOP: f32 = 8.0;

/// An unsettled fine-tick wheel gesture on a snapping scroller.
#[derive(Clone, Copy, Debug)]
pub(super) struct WheelSnap {
    key: exact_kernel::NodeKey,
    start: (f32, f32),
    last: (f32, f32),
    min: (f32, f32),
    max: (f32, f32),
    moved: (bool, bool),
    due: f64,
}

/// A browser's wheel notch, in points (the Linux host's `input::LINE`): a whole
/// notch on an idle scroller moves one snap position at once; finer ticks
/// scroll freely and settle once the wheel goes quiet.
const WHEEL_NOTCH: f32 = 40.0;

/// How near a `proximity` snap's position must be to where the scroll would
/// rest: a third of the snapport.
pub const PROXIMITY: f32 = 1.0 / 3.0;

/// An area spanning `lo..hi` along the axis, in the coordinates of the
/// snapport's start, in a snapport `port` long whose start may rest in
/// `range`. One no longer than the snapport rests at one offset, its start,
/// centre or end at the snapport's; a longer one at any offset while it
/// covers the snapport (§5.2.2).
pub fn area(
    lo: f32,
    hi: f32,
    align: SnapAlign,
    stop: bool,
    port: f32,
    range: (f32, f32),
) -> SnapArea {
    let wide = hi - lo > port;
    let at = match align {
        _ if wide => lo,
        SnapAlign::Start => lo,
        SnapAlign::Center => (lo + hi - port) / 2.0,
        SnapAlign::End => hi - port,
    };
    let from = at.clamp(range.0, range.1.max(range.0));
    let to = if wide {
        (hi - port).clamp(from, range.1.max(from))
    } else {
        from
    };
    SnapArea {
        from,
        to,
        stop,
        id: 0,
    }
}

/// Where a scroll released at `release` and projected to rest at `projected`
/// comes to rest among `areas`, or `None` to rest where it would: the
/// position nearest the projection (the smaller of two as near); then short
/// of the first `always` position past the release in the way it goes
/// (§6.2); and for `proximity`, only within a third of the snapport of the
/// projection. [`flicked`] for a flick.
pub fn rest(
    areas: &[SnapArea],
    projected: f32,
    release: f32,
    mandatory: bool,
    port: f32,
) -> Option<f32> {
    flicked(areas, projected, release, (release, 0.0), mandatory, port)
}

/// [`rest`] for a flick `way` (1 as the offset grows, -1 back) whose drag
/// began at `start`: before the `always` hold, a rest back where the drag
/// began, or behind it, goes on to the first position past `start` that way,
/// as a UIKit pager's flick does.
pub fn flicked(
    areas: &[SnapArea],
    projected: f32,
    release: f32,
    (start, way): (f32, f32),
    mandatory: bool,
    port: f32,
) -> Option<f32> {
    let at = |a: &SnapArea| projected.clamp(a.from, a.to);
    let mut rest = areas.iter().map(at).min_by(|a, b| {
        (a - projected)
            .abs()
            .total_cmp(&(b - projected).abs())
            .then(a.total_cmp(b))
    })?;
    // A move under half a pixel is none: positions from layout are fractional.
    let e = 0.5;
    if way != 0.0 && way * (rest - start) <= e {
        let next = (areas.iter())
            .map(|a| (start + way * 2.0 * e).clamp(a.from, a.to))
            .filter(|p| way * (p - start) > e)
            .min_by(|a, b| (way * a).total_cmp(&(way * b)));
        rest = next.unwrap_or(rest);
    }
    let stops = || areas.iter().filter(|a| a.stop);
    if rest > release + e {
        let first = stops()
            .filter_map(|a| {
                (a.from > release + e)
                    .then_some(a.from)
                    .or((a.to > release + e).then_some(a.to))
            })
            .min_by(f32::total_cmp);
        rest = first.map_or(rest, |stop| rest.min(stop));
    } else if rest < release - e {
        let first = stops()
            .filter_map(|a| {
                (a.to < release - e)
                    .then_some(a.to)
                    .or((a.from < release - e).then_some(a.from))
            })
            .max_by(f32::total_cmp);
        rest = first.map_or(rest, |stop| rest.max(stop));
    }
    (mandatory || (rest - projected).abs() <= port * PROXIMITY).then_some(rest)
}

/// CSS Scroll Snap 1 §5.5, a re-snap after layout changes: a scroller that
/// `rested` on an area (its node and the position) and is now at `current`
/// goes where that area now rests, when layout moved it; when the area went
/// and the scroller is still where it rested, to the position nearest.
/// `None`: it stays.
pub fn resnap(
    areas: &[SnapArea],
    rested: (ViewId, f32),
    current: f32,
    mandatory: bool,
    port: f32,
) -> Option<f32> {
    let (id, at) = rested;
    if id == 0 {
        return None;
    }
    let kept: Vec<SnapArea> = areas.iter().copied().filter(|a| a.id == id).collect();
    if kept.is_empty() {
        return ((current - at).abs() < 0.5)
            .then(|| rest(areas, current, current, mandatory, port))
            .flatten();
    }
    rest(&kept, current, current, true, port).filter(|p| (p - current).abs() > 0.5)
}

impl<D: DataSource> Presenter<D> {
    /// A new command's destination belongs to the current layout on each axis it moves.
    pub(super) fn snap_requested(&mut self, id: ViewId, from: (f32, f32), to: (f32, f32)) {
        if self.snap_layout.axes.is_empty() {
            return;
        }
        let stamp = self.host.kernel().layout_stamp();
        for (key, moved) in [((id, false), from.0 != to.0), ((id, true), from.1 != to.1)] {
            if moved
                && self
                    .snap_layout
                    .axes
                    .get(&key)
                    .is_some_and(|a| a.stamp != stamp)
            {
                self.snap_layout.axes.remove(&key);
                self.snap_layout.scanned = None;
            }
        }
    }

    /// A commit outside a snap container and its ancestors leaves its saved areas valid.
    pub(super) fn advance_snap_layout(&mut self) {
        let k = self.host.kernel();
        let stamp = k.layout_stamp();
        let current = (stamp, self.viewport, self.brush.scale);
        let Some((previous, viewport, scale)) = self.snap_layout.checked.replace(current) else {
            return;
        };
        if self.snap_layout.axes.is_empty()
            || previous == stamp
            || self.viewport != viewport
            || self.brush.scale != scale
            || self.host.content_region().is_some()
        {
            return;
        }
        let Some(changes) = self.host.row_dirty.known_nodes() else {
            return;
        };
        let mut changed = IdSet::default();
        let mut containing_changes = IdSet::default();
        let mut discover = false;
        for key in changes {
            let Some(n) = k.node_by_key(key) else { return };
            if changed.insert(n.id) {
                discover |= n.style.scroll_snap_type.axes() != (false, false);
                let mut at = Some(n.id);
                while let Some(id) = at {
                    if !containing_changes.insert(id) {
                        break;
                    }
                    at = k.node(id).and_then(|n| n.parent);
                }
            }
        }
        for (&(id, _), axis) in &mut self.snap_layout.axes {
            if axis.stamp != previous || containing_changes.contains(&id) {
                continue;
            }
            let mut ancestors =
                std::iter::successors(Some(id), |&id| k.node(id).and_then(|n| n.parent));
            if !ancestors.any(|id| changed.contains(&id)) {
                axis.stamp = stamp;
            }
        }
        if !discover
            && self.snap_layout.scanned == Some(previous)
            && self.snap_layout.axes.values().all(|a| a.stamp == stamp)
        {
            self.snap_layout.scanned = Some(stamp);
        }
    }

    pub(super) fn use_touch_snap(&mut self, id: ViewId, vertical: bool) {
        let key = (id, vertical);
        if let Some(offered) = self.snap_layout.offered.remove(&key) {
            self.snap_layout.axes.insert(key, offered);
        }
    }

    /// The target uses the density of the snap areas supplied at touch-down.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn reader_snap_target(
        &mut self,
        id: ViewId,
        vertical: bool,
        pixels: f32,
        landing: f32,
        scale: f32,
    ) {
        self.use_touch_snap(id, vertical);
        self.snap_destination(
            id,
            vertical,
            pixels / self.snap_layout.touch_scale,
            landing / scale,
        );
    }

    /// Keep the destination in the layout the reader used for its release. A smaller
    /// range can end the fling before the reader reaches that offset.
    pub(crate) fn snap_destination(
        &mut self,
        id: ViewId,
        vertical: bool,
        target: f32,
        landing: f32,
    ) {
        if let Some(axis) = self.snap_layout.axes.get_mut(&(id, vertical)) {
            axis.release = Some((target, landing));
        }
    }

    pub(super) fn snap_motion_landed(&mut self, id: ViewId) {
        let at = self.scroll_of(id);
        let stamp = self.host.kernel().layout_stamp();
        let mut range = None;
        for (vertical, current) in [(false, at.0), (true, at.1)] {
            let Some(axis) = self.snap_layout.axes.get_mut(&(id, vertical)) else {
                continue;
            };
            let Some((to, landing)) = axis.release.take() else {
                continue;
            };
            if axis.area_at(to).is_none() {
                continue;
            }
            if (current - landing).abs() < 0.5 {
                axis.clamped = Some((current, to));
                continue;
            }
            if axis.stamp == stamp || (to <= current && axis.before_clamp(current) == current) {
                continue;
            }
            let max = *range
                .get_or_insert_with(|| self.scroll_range(id, &self.collection_scroll_limits()));
            if (current - if vertical { max.1 } else { max.0 }).abs() < 0.5 {
                self.snap_layout
                    .axes
                    .get_mut(&(id, vertical))
                    .unwrap()
                    .clamped = Some((current, to));
            }
        }
    }

    fn snap_busy(&self, id: ViewId) -> bool {
        self.pending_model_scroll(id).is_some() || self.snap_input_busy(id)
    }

    fn snap_input_busy(&self, id: ViewId) -> bool {
        let inside = |node| node == id || self.is_inside(node, id);
        self.snap_input_active(id)
            || self.wheel_snap_pending(id)
            || self.gliding(id)
            || self.zoom_animating(id)
            || self
                .control_contact
                .is_some_and(|(node, _, _)| inside(node))
            || self.brush.scrollbars.drag.is_some_and(|(key, _, _)| {
                self.host
                    .kernel()
                    .node_by_key(key)
                    .is_some_and(|n| n.id == id)
            })
            || !self.reader_dragging()
                && [self.contact_hit(), self.pointer_held]
                    .into_iter()
                    .flatten()
                    .any(|key| {
                        self.host
                            .kernel()
                            .node_by_key(key)
                            .is_some_and(|n| inside(n.id))
                    })
    }

    fn changed_snap_ids(&self) -> Vec<ViewId> {
        let kernel = self.host.kernel();
        self.host
            .snap_changes
            .pending
            .iter()
            .filter_map(|&key| {
                let node = kernel.node_by_key(key)?;
                if !node.style.scroll_snap_type.mandatory()
                    || self.host.route_visibility(node.id).0
                    || self.snap_input_busy(node.id)
                    || (self.pending_model_scroll(node.id).is_some()
                        && self.snap_model_target(node.id).is_none())
                {
                    return None;
                }
                let owns = |o| matches!(o, Overflow::Scroll | Overflow::Auto | Overflow::Hidden);
                let (x, y) = effective_overflow(&node);
                if !owns(x) && !owns(y) {
                    return None;
                }
                let mut at = Some(node);
                while let Some(n) = at {
                    if n.style.display == Display::None {
                        return None;
                    }
                    at = n.parent.and_then(|id| kernel.node(id));
                }
                Some(node.id)
            })
            .collect()
    }

    /// A gesture keeps the areas its release uses until it rests. Other scrollers can
    /// follow their areas in the first picture of the new layout.
    pub(crate) fn snap_probe_needed(&self) -> bool {
        let stamp = self.host.kernel().layout_stamp();
        !self.changed_snap_ids().is_empty()
            || self.snap_layout.axes.iter().any(|(&(id, vertical), axis)| {
                let at = self.scroll_of(id);
                axis.stamp != stamp
                    && axis.absent != Some(stamp)
                    && !self.snap_busy(id)
                    && (self
                        .host
                        .kernel()
                        .node(id)
                        .is_some_and(|n| n.style.scroll_snap_type.mandatory())
                        || axis.rested(if vertical { at.1 } else { at.0 }).is_some())
            })
    }

    /// Read the new picture's boxes before drawing it. Input still addresses the
    /// acknowledged picture, whose scroll range can already be too short for this rest.
    pub(crate) fn resnap_before_paint(&mut self) {
        let stamp = self.host.kernel().layout_stamp();
        let mut moves = BTreeMap::new();
        let changed = self.changed_snap_ids();
        let mut axes: BTreeSet<_> = self.snap_layout.axes.keys().copied().collect();
        for &id in &changed {
            let enabled = self
                .host
                .kernel()
                .node(id)
                .unwrap()
                .style
                .scroll_snap_type
                .axes();
            axes.extend(
                [(id, false), (id, true)]
                    .into_iter()
                    .filter(|&(_, vertical)| if vertical { enabled.1 } else { enabled.0 }),
            );
        }
        for (id, vertical) in axes {
            let fresh = changed.contains(&id);
            let old = self.snap_layout.axes.get(&(id, vertical));
            if !fresh
                && (self.snap_busy(id)
                    || old.is_none_or(|a| a.stamp == stamp || a.absent == Some(stamp)))
            {
                continue;
            }
            let model = self.snap_model_target(id);
            let at = model.unwrap_or_else(|| self.scroll_of(id));
            let current = if vertical { at.1 } else { at.0 };
            let rested = old
                .filter(|_| model.is_none())
                .and_then(|a| a.rested(current));
            if rested.is_none()
                && !self
                    .host
                    .kernel()
                    .node(id)
                    .is_some_and(|n| n.style.scroll_snap_type.mandatory())
            {
                continue;
            }
            let Some(snap) = self.snap_at(id, vertical, None, Geometry::Current) else {
                continue;
            };
            let target = match rested {
                Some((area, _)) => resnap(
                    &snap.areas,
                    (area, current),
                    current,
                    snap.mandatory,
                    snap.port,
                ),
                None if snap.mandatory => rest(&snap.areas, current, current, true, snap.port),
                None => None,
            };
            if let Some(to) = target {
                let next = moves.entry(id).or_insert(at);
                if vertical {
                    next.1 = to;
                } else {
                    next.0 = to;
                }
            }
            self.snap_layout.axes.insert(
                (id, vertical),
                AxisLayout::new(stamp, snap, rested.map(|r| r.0)),
            );
        }
        for (id, to) in moves {
            let from = self.scroll_of(id);
            if !self.retarget_snap_model(id, to) && from != to {
                self.set_scroll_offset(id, from, to, Completion::Instant);
            }
        }
        for id in changed {
            if let Some(node) = self.host.kernel().node(id) {
                self.host.snap_changes.pending.remove(&node.key);
            }
        }
    }

    /// Keep the layout in which a later offset comes to rest. Reader samples need
    /// no area walk and can arrive right up to the next layout.
    pub(crate) fn remember_snap_layout(&mut self) {
        let declared = *self.snap_layout.declared.get_or_insert_with(|| {
            self.host.runner().plan().bindings.iter().any(|b| {
                b.kind == exact_plan::BindingKind::Style
                    && b.id == exact_kernel::StyleId::ScrollSnapType as u16
            })
        });
        if !declared && self.host.content_region().is_none() {
            return;
        }
        let stamp = self.host.kernel().layout_stamp();
        if self.snap_layout.scanned == Some(stamp)
            && self.snap_layout.axes.iter().all(|(&(id, _), a)| {
                a.stamp == stamp || a.absent == Some(stamp) || self.snap_busy(id)
            })
        {
            return;
        }
        let kernel = self.host.kernel();
        self.snap_layout.axes.retain(|&(id, vertical), _| {
            kernel.node(id).is_some_and(|n| {
                let axes = n.style.scroll_snap_type.axes();
                if vertical {
                    axes.1
                } else {
                    axes.0
                }
            })
        });
        let ids: Vec<_> = self
            .boxes
            .iter()
            .filter(|b| b.scroll.is_some())
            .filter_map(|b| {
                let axes = kernel.node(b.id)?.style.scroll_snap_type.axes();
                (axes.0 || axes.1).then_some((b.id, axes))
            })
            .collect();
        if self.snap_layout.scanned != Some(stamp) {
            // A hidden scroller keeps its last geometry until it is shown again.
            for axis in self.snap_layout.axes.values_mut() {
                axis.absent = Some(stamp);
            }
        }
        for (id, axes) in ids {
            for (vertical, enabled) in [(false, axes.0), (true, axes.1)] {
                if !enabled {
                    continue;
                }
                if let Some(a) = self.snap_layout.axes.get_mut(&(id, vertical)) {
                    a.absent = None;
                }
                if self
                    .snap_layout
                    .axes
                    .get(&(id, vertical))
                    .is_some_and(|a| a.stamp == stamp || self.snap_busy(id))
                {
                    continue;
                }
                if let Some(snap) = self.snap_at(id, vertical, None, Geometry::Current) {
                    let preferred = self
                        .snap_layout
                        .axes
                        .get(&(id, vertical))
                        .and_then(|a| a.preferred);
                    self.snap_layout
                        .axes
                        .insert((id, vertical), AxisLayout::new(stamp, snap, preferred));
                }
            }
        }
        self.snap_layout.scanned = Some(stamp);
        self.snap_layout.checked = Some((stamp, self.viewport, self.brush.scale));
    }

    /// Scroller `id`'s snap along y when `vertical`, else along x; `None`
    /// when it does not snap along that axis. Its areas are the boxes with a
    /// `scroll-snap-align` along the axis inside it and in no scroll
    /// container nearer them, each grown by its `scroll-margin` (§4.3); the
    /// snapport is its padding box less `scroll-padding` (§4.1). A
    /// virtualized list adds the rows it has not mounted, up to 64 either way
    /// of the one at its offset (`Runner::collection_snap_areas`).
    pub fn snap(&mut self, id: ViewId, vertical: bool) -> Option<Snap> {
        self.snap_at(id, vertical, None, Geometry::Presented)
    }

    /// [`Presenter::snap`], with a virtualized list's unmounted rows read
    /// around `aim`'s second offset instead of the list's own, and the first
    /// unmounted `always` row past its first offset toward its second
    /// (`Runner::collection_snap_stop`): for a release at the first whose
    /// rest is near the second.
    fn snap_at(
        &mut self,
        id: ViewId,
        vertical: bool,
        aim: Option<(f32, f32)>,
        geometry: Geometry,
    ) -> Option<Snap> {
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        let kind = node.style.scroll_snap_type;
        let axes = kind.axes();
        if !(if vertical { axes.1 } else { axes.0 }) {
            return None;
        }
        let [bt, br, bb, bl] = node.style.border_widths();
        let r = &node.style.rare;
        let padding = [
            r.scroll_padding_top.clone(),
            r.scroll_padding_right.clone(),
            r.scroll_padding_bottom.clone(),
            r.scroll_padding_left.clone(),
        ];
        // The area walk: a nested scroll container captures its own areas.
        let mut found = Vec::new();
        let mut walk: Vec<ViewId> = node.children();
        while let Some(c) = walk.pop() {
            let Some(n) = kernel.node(c) else { continue };
            if let Some(align) = n.style.scroll_snap_align.along(vertical) {
                let m = &n.style.rare;
                let grow = if vertical {
                    [&m.scroll_margin_top, &m.scroll_margin_bottom]
                } else {
                    [&m.scroll_margin_left, &m.scroll_margin_right]
                }
                .map(|d| d.at_basis(&kernel.env_of(c), 0.0));
                let stop = n.style.scroll_snap_stop == exact_kernel::ScrollSnapStop::Always;
                found.push((c, align, stop, grow));
            }
            let (ox, oy) = effective_overflow(&n);
            // `hidden` is a scroll container; `clip` is not (CSS Overflow 3 §3.1).
            let owns = |o| matches!(o, Overflow::Scroll | Overflow::Auto | Overflow::Hidden);
            if !owns(ox) && !owns(oy) {
                walk.extend(n.children());
            }
        }
        let mandatory = kind.mandatory();
        let limits = self.collection_scroll_limits();
        let bounds = if geometry == Geometry::Current {
            self.brush
                .scroll_bounds(
                    kernel,
                    self.host.content_region(),
                    &node,
                    limits.get(&id).copied(),
                    &|id| self.host.presented(id),
                    self.host.scaled_presented(),
                )
                .max
        } else {
            self.scroller(id, &limits)?.max
        };
        let off = self.scroll_of(id);
        let b = self.reveal_box(id, geometry)?;
        let painted = b.scroll.unwrap_or(off);
        let (at, length, before, after, max, offset) = if vertical {
            (b.rect.1, b.rect.3, bt, bb, bounds.1, (painted.1, off.1))
        } else {
            (b.rect.0, b.rect.2, bl, br, bounds.0, (painted.0, off.0))
        };
        let env = self.host.kernel().env_of(id);
        let scrollport = (length - before - after).max(0.0);
        let pad = |i: usize| padding[i].at_basis(&env, scrollport).max(0.0);
        let (pad_before, pad_after) = if vertical {
            (pad(0), pad(2))
        } else {
            (pad(3), pad(1))
        };
        // Positions are where the snapport's start is in the scroller's
        // border box: an offset plus `lead`.
        let lead = before + pad_before;
        let port = (scrollport - pad_before - pad_after).max(1.0);
        let range = (lead, max + lead);
        let mut areas = Vec::new();
        for (c, align, stop, grow) in found {
            let Some(a) = self.reveal_box(c, geometry) else {
                continue;
            };
            let (lo, len) = if vertical {
                (a.rect.1, a.rect.3)
            } else {
                (a.rect.0, a.rect.2)
            };
            let lo = lo - at + offset.0;
            let at = area(lo - grow[0], lo + len + grow[1], align, stop, port, range);
            areas.push(SnapArea { id: c, ..at });
        }
        let horizontal = !vertical;
        let list = (self.host.runner().collection_data())
            .iter()
            .any(|&(view, _, h)| view == id && h == horizontal);
        if list {
            let around = f64::from(aim.map_or(offset.1, |a| a.1) + lead);
            let reach = 8.0 * f64::from(port);
            let rows = self.host.runner().collection_snap_areas(
                id,
                (
                    around,
                    [around - reach, around + reach + f64::from(port)],
                    64,
                ),
                [f64::from(port), f64::from(range.0), f64::from(range.1)],
            );
            areas.extend(rows.into_iter().map(|(from, to, stop)| SnapArea {
                from: from as f32,
                to: to as f32,
                stop,
                id: 0,
            }));
            let snapport = [f64::from(port), f64::from(range.0), f64::from(range.1)];
            let release = aim.map(|(release, rest)| [release + lead, rest + lead].map(f64::from));
            if let Some(stop) =
                release.and_then(|r| self.host.runner().collection_snap_stop(id, r, snapport))
            {
                areas.push(SnapArea {
                    from: stop as f32,
                    to: stop as f32,
                    stop: true,
                    id: 0,
                });
            }
        }
        for a in &mut areas {
            a.from -= lead;
            a.to -= lead;
        }
        Some(Snap {
            mandatory,
            port,
            areas,
        })
    }

    fn wheel_now(&self) -> f64 {
        self.live_time().unwrap_or_else(|| self.host.now())
    }

    fn wheel_snap_preempted(&self, id: ViewId) -> bool {
        let inside = |node| node == id || self.is_inside(node, id);
        self.pending_model_scroll(id).is_some()
            || self.gliding(id)
            || self.zoom_animating(id)
            || self.scroll_held(id)
            || self
                .control_contact
                .is_some_and(|(node, _, _)| inside(node))
            || self.brush.scrollbars.drag.is_some_and(|(key, _, _)| {
                self.host
                    .kernel()
                    .node_by_key(key)
                    .is_some_and(|n| n.id == id)
            })
            || !self.reader_dragging()
                && [self.contact_hit(), self.pointer_held]
                    .into_iter()
                    .flatten()
                    .any(|key| {
                        self.host
                            .kernel()
                            .node_by_key(key)
                            .is_some_and(|n| inside(n.id))
                    })
    }

    fn wheel_snap_valid(&self, id: ViewId, w: &WheelSnap) -> bool {
        self.scroll_of(id) == w.last
            && self.host.kernel().node(id).is_some_and(|n| n.key == w.key)
            && !self.wheel_snap_preempted(id)
    }

    /// Whether a layout change since `id`'s last snap requires `resnap_before_paint`
    /// before its scroll completion fires.
    pub(super) fn snap_resnap_pending(&self, id: ViewId) -> bool {
        let stamp = self.host.kernel().layout_stamp();
        let at = self.scroll_of(id);
        [(false, at.0), (true, at.1)]
            .into_iter()
            .any(|(vertical, cur)| {
                self.snap_layout
                    .axes
                    .get(&(id, vertical))
                    .is_some_and(|axis| {
                        axis.stamp != stamp
                            && axis.absent != Some(stamp)
                            && (self
                                .host
                                .kernel()
                                .node(id)
                                .is_some_and(|n| n.style.scroll_snap_type.mandatory())
                                || axis.rested(cur).is_some())
                    })
            })
    }

    /// Carry an unsettled fine-tick run's coordinates across an anchor or clamp rebase.
    pub(super) fn rebase_wheel_snap(&mut self, id: ViewId, was: (f32, f32), now: (f32, f32)) {
        let Some(w) = self.wheel_snap.get_mut(&id) else {
            return;
        };
        let shift = |v: f32, from: f32, to: f32| if v == from { to } else { v + to - from };
        w.start = (
            shift(w.start.0, was.0, now.0),
            shift(w.start.1, was.1, now.1),
        );
        w.last = (shift(w.last.0, was.0, now.0), shift(w.last.1, was.1, now.1));
        w.min = (shift(w.min.0, was.0, now.0), shift(w.min.1, was.1, now.1));
        w.max = (shift(w.max.0, was.0, now.0), shift(w.max.1, was.1, now.1));
    }

    /// Whether scroller `id` has an active fine-tick wheel gesture awaiting settle.
    pub(super) fn wheel_snap_pending(&self, id: ViewId) -> bool {
        self.wheel_snap
            .get(&id)
            .is_some_and(|w| self.wheel_snap_valid(id, w))
    }

    /// Whether any snapping scroller is awaiting its fine-tick wheel settle.
    pub(super) fn has_active_wheel_snap(&self) -> bool {
        self.wheel_snap
            .iter()
            .any(|(&id, w)| self.wheel_snap_valid(id, w))
    }

    /// Earliest deadline at which an unsettled fine-tick wheel gesture snaps.
    pub(crate) fn wheel_snap_due(&self) -> Option<f64> {
        self.wheel_snap
            .iter()
            .filter(|(&id, w)| self.wheel_snap_valid(id, w))
            .map(|(_, w)| w.due)
            .reduce(f64::min)
    }

    /// A wheel moved scroller `id` from `from` to `to` with input delta `delta`.
    /// A whole notch (`>= WHEEL_NOTCH`) on an idle scroller moves one snap
    /// position at once, as a browser's mouse wheel does. Finer ticks — and
    /// later ticks during an open fine-tick run — scroll freely and settle once
    /// the wheel goes quiet for `WHEEL_SNAP_QUIET_MS`. Returns the new offset
    /// and whether a fine-tick wheel gesture remains open on `id`.
    pub(super) fn wheel_snapped(
        &mut self,
        id: ViewId,
        from: (f32, f32),
        to: (f32, f32),
        delta: (f32, f32),
    ) -> ((f32, f32), bool) {
        let Some((key, axes)) = self
            .host
            .kernel()
            .node(id)
            .map(|n| (n.key, n.style.scroll_snap_type.axes()))
        else {
            return (to, false);
        };
        let active = self
            .wheel_snap
            .get(&id)
            .copied()
            .filter(|w| w.key == key && w.last == from);
        let mut rests = to;
        let mut fine = (false, false);
        let mut snapped_notch = false;
        for vertical in [false, true] {
            if !(if vertical { axes.1 } else { axes.0 }) {
                continue;
            }
            let (target, raw, release) = if vertical {
                (&mut rests.1, delta.1, from.1)
            } else {
                (&mut rests.0, delta.0, from.0)
            };
            if *target == release {
                continue;
            }
            if active.is_none() && raw.abs() >= WHEEL_NOTCH {
                snapped_notch = true;
                if let Some(snap) = self.snap(id, vertical) {
                    let way = raw.signum();
                    if let Some(at) = flicked(
                        &snap.areas,
                        *target,
                        release,
                        (release, way),
                        snap.mandatory,
                        snap.port,
                    ) {
                        *target = at;
                    }
                }
            } else if vertical {
                fine.1 = true;
            } else {
                fine.0 = true;
            }
        }
        if fine.0 || fine.1 {
            let due = self.wheel_now() + WHEEL_SNAP_QUIET_MS;
            let next = match active {
                Some(w) => WheelSnap {
                    key,
                    start: (
                        if !w.moved.0 && fine.0 {
                            from.0
                        } else {
                            w.start.0
                        },
                        if !w.moved.1 && fine.1 {
                            from.1
                        } else {
                            w.start.1
                        },
                    ),
                    last: rests,
                    min: (
                        if w.moved.0 {
                            w.min.0.min(rests.0)
                        } else {
                            from.0.min(rests.0)
                        },
                        if w.moved.1 {
                            w.min.1.min(rests.1)
                        } else {
                            from.1.min(rests.1)
                        },
                    ),
                    max: (
                        if w.moved.0 {
                            w.max.0.max(rests.0)
                        } else {
                            from.0.max(rests.0)
                        },
                        if w.moved.1 {
                            w.max.1.max(rests.1)
                        } else {
                            from.1.max(rests.1)
                        },
                    ),
                    moved: (w.moved.0 || fine.0, w.moved.1 || fine.1),
                    due,
                },
                None => WheelSnap {
                    key,
                    start: from,
                    last: rests,
                    min: (from.0.min(rests.0), from.1.min(rests.1)),
                    max: (from.0.max(rests.0), from.1.max(rests.1)),
                    moved: fine,
                    due,
                },
            };
            self.wheel_snap.insert(id, next);
            (rests, true)
        } else if let Some(mut w) = active.filter(|_| !snapped_notch) {
            // A perpendicular tick on an unsnapped axis keeps the open snap run alive.
            w.last = rests;
            self.wheel_snap.insert(id, w);
            (rests, true)
        } else {
            self.wheel_snap.remove(&id);
            (rests, false)
        }
    }

    /// Settle any fine-tick wheel gestures whose quiet window has elapsed.
    pub(super) fn settle_wheel_snaps(&mut self) {
        if self.wheel_snap.is_empty() {
            return;
        }
        let ids: Vec<ViewId> = self.wheel_snap.keys().copied().collect();
        for id in ids {
            let Some(w) = self.wheel_snap.get(&id).copied() else {
                continue;
            };
            if !self.wheel_snap_valid(id, &w) {
                self.wheel_snap.remove(&id);
                continue;
            }
            if w.due > self.wheel_now() {
                continue;
            }
            let at = self.scroll_of(id);
            let stamp = self.host.kernel().layout_stamp();
            let mut target = at;
            let mut chosen: [Option<Snap>; 2] = [None, None];
            for vertical in [false, true] {
                if !(if vertical { w.moved.1 } else { w.moved.0 }) {
                    continue;
                }
                let Some(snap) = self.snap(id, vertical) else {
                    continue;
                };
                let (start, cur, lo, hi, out) = if vertical {
                    (w.start.1, at.1, w.min.1, w.max.1, &mut target.1)
                } else {
                    (w.start.0, at.0, w.min.0, w.max.0, &mut target.0)
                };
                let total = cur - start;
                let pullback = if total > 0.0 { hi - cur } else { cur - lo };
                let way = if total.abs() > 0.5 && pullback < WHEEL_PULLBACK_SLOP {
                    total.signum()
                } else {
                    0.0
                };
                if let Some(pos) = flicked(
                    &snap.areas,
                    cur,
                    cur,
                    (start, way),
                    snap.mandatory,
                    snap.port,
                ) {
                    *out = pos;
                }
                chosen[usize::from(vertical)] = Some(snap);
            }
            // Record the geometry used to choose this rest so a later layout
            // commit follows the chosen area rather than an earlier layout's.
            for (axis, snap) in chosen.into_iter().enumerate() {
                if let Some(snap) = snap {
                    let vertical = axis == 1;
                    let dest = if vertical { target.1 } else { target.0 };
                    let mut layout = AxisLayout::new(stamp, snap, None);
                    if target != at {
                        layout.release = Some((dest, dest));
                    }
                    self.snap_layout.axes.insert((id, vertical), layout);
                }
            }
            if target != at {
                if self.reveal_glides(None) {
                    self.scroll_end_moved(id, Completion::Glide);
                    self.wheel_snap.remove(&id);
                    self.begin_glide(id, at, target);
                    if self.host.collection(id).is_some() {
                        self.queue_collections();
                    }
                    self.dirty = true;
                } else {
                    self.set_scroll_offset(id, at, target, Completion::Instant);
                    self.wheel_snap.remove(&id);
                    self.snap_motion_landed(id);
                }
            } else {
                self.wheel_snap.remove(&id);
                self.dirty |= self.snap_probe_needed();
            }
        }
    }

    /// Each of scrollers `ids`' snap, for a reader that aims its fling
    /// ([`Presenter::snap`]): per scroller, flags (bit 0 it snaps along x,
    /// bit 1 along y, bit 2 `mandatory`), then for x and then y the
    /// snapport's length as float bits, a count, and per area its least and
    /// greatest offsets as float bits and 1 for `scroll-snap-stop: always`,
    /// in the reader's pixels. An axis it does not snap along has a zero
    /// length and no areas. Flag bit 3: it is a virtualized list, whose rows
    /// past these the reader asks for at its release
    /// ([`Presenter::reader_snaps_around`]).
    pub fn reader_snaps(&mut self, ids: &[ViewId], scale: f32) -> Vec<i32> {
        self.snap_layout.offered.clear();
        self.snap_layout.touch_scale = scale;
        let mut out = Vec::new();
        for &id in ids {
            let snaps = [self.snap(id, false), self.snap(id, true)];
            let mandatory = snaps.iter().flatten().any(|s| s.mandatory);
            let list = (self.host.runner().collection_data())
                .iter()
                .any(|&(view, _, _)| view == id);
            out.push(
                i32::from(snaps[0].is_some())
                    | i32::from(snaps[1].is_some()) << 1
                    | i32::from(mandatory) << 2
                    | i32::from(list) << 3,
            );
            let stamp = self.host.kernel().layout_stamp();
            for (axis, snap) in snaps.into_iter().enumerate() {
                let (port, areas) = snap
                    .as_ref()
                    .map_or((0.0, &[][..]), |s| (s.port, &s.areas[..]));
                out.extend([(port * scale).to_bits() as i32, areas.len() as i32]);
                for a in areas {
                    out.extend([
                        (a.from * scale).to_bits() as i32,
                        (a.to * scale).to_bits() as i32,
                        i32::from(a.stop),
                    ]);
                }
                // A finger that catches a fling uses the areas it was just shown.
                if self.display.shows_epoch(stamp.1) {
                    if let Some(snap) = snap {
                        let key = (id, axis == 1);
                        let preferred = self
                            .snap_layout
                            .axes
                            .get(&key)
                            .and_then(|old| old.preferred);
                        self.snap_layout
                            .offered
                            .insert(key, AxisLayout::new(stamp, snap, preferred));
                    }
                }
            }
        }
        out
    }

    /// Virtualized list `id`'s snap areas along y when `vertical`, else x,
    /// for a reader whose release at `release` projects a rest near
    /// `projected` (offsets, in its pixels), past the rows it was told of at
    /// the touch ([`Presenter::reader_snaps`]): a count, then per area its
    /// least and greatest offsets as float bits and 1 for `always`. The rows
    /// it has not mounted are read around the projection, with the first
    /// `always` one past the release toward it.
    pub fn reader_snaps_around(
        &mut self,
        id: ViewId,
        vertical: bool,
        release: f32,
        projected: f32,
        scale: f32,
    ) -> Vec<i32> {
        let areas = self
            .snap_at(
                id,
                vertical,
                Some((release / scale, projected / scale)),
                Geometry::Presented,
            )
            .map(|s| s.areas)
            .unwrap_or_default();
        let mut out = vec![areas.len() as i32];
        for a in &areas {
            out.extend([
                (a.from * scale).to_bits() as i32,
                (a.to * scale).to_bits() as i32,
                i32::from(a.stop),
            ]);
        }
        out
    }
}

#[cfg(test)]
#[path = "snap_tests.rs"]
mod tests;
