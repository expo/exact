//! CSS Scroll Snap 1 on the Linux family (LLP 1008 §9, "Scroll snap", the
//! definition every native host shares): along one axis, the offsets a
//! scroller's snap areas rest at, and which of them a scroll that ends rests
//! at. Android's reader aims its fling with them ([`Presenter::reader_snaps`],
//! `Scrolling.kt`'s `snapRest`); a desktop wheel snaps here (`wheel.rs`).
//! Apple's is `host/apple/Sources/ExactKit/ScrollSnap.swift`.
use super::*;
use exact_kernel::style::snap::SnapAlign;

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

/// Where a wheel would have put a scroller unsnapped, and where it put it.
pub(super) type WheelFree = ((f32, f32), (f32, f32));

/// A browser's wheel notch, in points (`crate::input::LINE`): ticks that add
/// up to one are a flick.
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
    rest(&kept, current, current, true, port).filter(|p| (p - at).abs() > 0.5)
}

impl<D: DataSource> Presenter<D> {
    /// [`resnap`] on each scroller the presenter moves whose snap is
    /// `mandatory`, once a commit is laid out and nothing scrolls. A scroller
    /// Android's reader moves is the reader's to place (LLP 1115.001 D7) and
    /// is left. The area a scroller rested on is the one this found it on
    /// after the last commit.
    pub(super) fn resnap_scrollers(&mut self) {
        if self.scroll_activity() {
            return;
        }
        let ids: Vec<ViewId> = (self.scroll.keys().copied())
            .filter(|&id| {
                !self.reader_knows(id)
                    && self
                        .host
                        .kernel()
                        .node(id)
                        .is_some_and(|n| n.style.scroll_snap_type.mandatory())
            })
            .collect();
        for id in ids {
            let at = self.scroll_of(id);
            let mut to = at;
            for vertical in [false, true] {
                let Some(snap) = self.snap(id, vertical) else {
                    continue;
                };
                let (current, next) = if vertical {
                    (at.1, &mut to.1)
                } else {
                    (at.0, &mut to.0)
                };
                let rested = self
                    .snap_rested
                    .get(&(id, vertical))
                    .copied()
                    .unwrap_or((0, 0.0));
                if let Some(p) = resnap(&snap.areas, rested, current, snap.mandatory, snap.port) {
                    *next = p;
                }
                let on = (snap.areas.iter())
                    .find(|a| a.id != 0 && *next >= a.from - 0.5 && *next <= a.to + 0.5);
                match on {
                    Some(a) => self.snap_rested.insert((id, vertical), (a.id, *next)),
                    None => self.snap_rested.remove(&(id, vertical)),
                };
            }
            if to != at {
                self.scroll_to_completion(id, to, Completion::Instant, None);
            }
        }
    }

    /// Scroller `id`'s snap along y when `vertical`, else along x; `None`
    /// when it does not snap along that axis. Its areas are the boxes with a
    /// `scroll-snap-align` along the axis inside it and in no scroll
    /// container nearer them, each grown by its `scroll-margin` (§4.3); the
    /// snapport is its padding box less `scroll-padding` (§4.1). A
    /// virtualized list adds the rows it has not mounted, up to 64 either way
    /// of the one at its offset (`Runner::collection_snap_areas`).
    pub fn snap(&mut self, id: ViewId, vertical: bool) -> Option<Snap> {
        self.snap_at(id, vertical, None)
    }

    /// [`Presenter::snap`], with a virtualized list's unmounted rows read
    /// around `aim`'s second offset instead of the list's own, and the first
    /// unmounted `always` row past its first offset toward its second
    /// (`Runner::collection_snap_stop`): for a release at the first whose
    /// rest is near the second.
    fn snap_at(&mut self, id: ViewId, vertical: bool, aim: Option<(f32, f32)>) -> Option<Snap> {
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        let kind = node.style.scroll_snap_type;
        let axes = kind.axes();
        if !(if vertical { axes.1 } else { axes.0 }) {
            return None;
        }
        let env = kernel.arena().env();
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
                .map(|d| d.at_basis(env, 0.0));
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
        let s = self.scroller(id, &limits)?;
        let b = self.box_of(id)?;
        let painted = b.scroll.unwrap_or(s.offset);
        let (at, length, before, after, max, offset) = if vertical {
            (b.rect.1, b.rect.3, bt, bb, s.max.1, (painted.1, s.offset.1))
        } else {
            (b.rect.0, b.rect.2, bl, br, s.max.0, (painted.0, s.offset.0))
        };
        let env = self.host.kernel().arena().env();
        let pad = |i: usize| padding[i].at_basis(env, length).max(0.0);
        let (pad_before, pad_after) = if vertical {
            (pad(0), pad(2))
        } else {
            (pad(3), pad(1))
        };
        // Positions are where the snapport's start is in the scroller's
        // border box: an offset plus `lead`.
        let lead = before + pad_before;
        let port = (length - before - after - pad_before - pad_after).max(1.0);
        let range = (lead, max + lead);
        let mut areas = Vec::new();
        for (c, align, stop, grow) in found {
            let Some(a) = self.box_of(c) else { continue };
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

    /// A wheel moved scroller `id` from `from` to `to`, within `max`: where
    /// it rests instead, on each axis it moved along and snaps along. Where
    /// the wheel would have put it goes on from there unsnapped while the
    /// scroller stays where the last wheel left it, so ticks too short to
    /// reach another position add up; ticks that add up to a wheel notch are
    /// a flick their way.
    pub(super) fn wheel_snapped(
        &mut self,
        id: ViewId,
        from: (f32, f32),
        to: (f32, f32),
        max: (f32, f32),
    ) -> (f32, f32) {
        let free = match self.wheel_free.get(&id) {
            Some(&(free, left)) if left == from => (
                (free.0 + to.0 - from.0).clamp(0.0, max.0.max(0.0)),
                (free.1 + to.1 - from.1).clamp(0.0, max.1.max(0.0)),
            ),
            _ => to,
        };
        let mut rests = to;
        for vertical in [false, true] {
            let (f, r, release) = if vertical {
                (free.1, &mut rests.1, from.1)
            } else {
                (free.0, &mut rests.0, from.0)
            };
            if f == release {
                continue;
            }
            let Some(snap) = self.snap(id, vertical) else {
                continue;
            };
            // Ticks that add up to a browser's wheel notch are a flick their
            // way: a notch moves a mandatory snap one position, as a
            // browser's wheel does; finer ticks add up to one first.
            let way = if (f - release).abs() >= WHEEL_NOTCH {
                (f - release).signum()
            } else {
                0.0
            };
            if let Some(at) = flicked(
                &snap.areas,
                f,
                release,
                (release, way),
                snap.mandatory,
                snap.port,
            ) {
                *r = at;
            }
            match (snap.areas.iter()).find(|a| a.id != 0 && *r >= a.from - 0.5 && *r <= a.to + 0.5)
            {
                Some(a) => self.snap_rested.insert((id, vertical), (a.id, *r)),
                None => self.snap_rested.remove(&(id, vertical)),
            };
        }
        // Once it moves, the next ticks add up from where it rests.
        let free = (
            if rests.0 != from.0 { rests.0 } else { free.0 },
            if rests.1 != from.1 { rests.1 } else { free.1 },
        );
        self.wheel_free.insert(id, (free, rests));
        rests
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
            for snap in &snaps {
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
            .snap_at(id, vertical, Some((release / scale, projected / scale)))
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
