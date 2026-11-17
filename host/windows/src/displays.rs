//! The displays and the pointer (LLP 1178 D1, D2, D6): this host's
//! `exact_painted::desktop::Displays`, which `@exact/window-management` reads.
//!
//! - Windows reports each monitor in physical pixels, and the web's space is
//!   CSS pixels with one origin. The monitors are laid out into it as
//!   Chromium lays them out (`ui/display/win/screen_win.cc`,
//!   `scaling_util.cc`, `ui/display/display_layout.cc`): each rectangle over
//!   its own scale, then placed against the neighbour it touches, the primary
//!   first, and moved apart where rounding made two overlap. The layout's
//!   arithmetic is `f32` because Chromium's is, and a monitor's scale is
//!   Chromium's ([`scale`]), so an app reads what a page in Chrome reads on
//!   the same machine.
//! - `clientRect`, `clientX` and `clientY` are not in that space. A window
//!   has one scale, its DPI's, so each is a physical position less the
//!   client area's origin, over the window's scale.
//! - `currentScreen` is `MonitorFromWindow`'s nearest monitor, which holds
//!   the largest part of the window; the pointer's is `MonitorFromPoint`'s.
//! - While a session reads the displays, a subclass of the window hears
//!   `WM_DISPLAYCHANGE`, `WM_DPICHANGED` and the work area's
//!   `WM_SETTINGCHANGE`, and compares the window's monitor after each
//!   `WM_WINDOWPOSCHANGED`, since a move between two monitors at one scale
//!   sends none of the first three; `UISettings` tells of the Text size
//!   setting.
//! - A run with no window has no monitors to measure against: its one
//!   display is the buffer it paints ([`install_headless`]).

use exact_painted::desktop::{Pointer, Screen, Screens};

#[cfg(windows)]
mod os;
#[cfg(windows)]
pub(crate) use os::install;

/// One monitor as Windows reports it to a per-monitor-aware process.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Monitor {
    /// `rcMonitor`, in physical pixels: left, top, right, bottom.
    pub(crate) rect: [i32; 4],
    /// `rcWork`: the monitor less the taskbar and other app bars.
    pub(crate) work: [i32; 4],
    /// Physical pixels to each CSS pixel of the screens' space ([`scale`]).
    pub(crate) scale: f32,
    /// `MONITORINFOF_PRIMARY`: the monitor at the origin.
    pub(crate) primary: bool,
    /// A panel built into the device, by its output technology.
    pub(crate) internal: bool,
    /// The monitor's name from its EDID, or empty.
    pub(crate) label: String,
}

/// A monitor's scale: its effective DPI over 96, times `text`, the system's
/// Text size setting (1 at its default), as Chromium's
/// `GetScaleFactorsForDPI` has it. This host applies the text size to type
/// and leaves the viewport in the DPI's pixels, so `clientRect`, `clientX`
/// and `clientY` do not change with it.
pub(crate) fn scale(dpi: u32, text: f32) -> f32 {
    dpi as f32 / 96. * text
}

/// `DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY`'s values for a panel built into
/// the device ([wingdi.h](https://learn.microsoft.com/windows/win32/api/wingdi/ne-wingdi-displayconfig_video_output_technology)).
/// A display driver reports a MIPI DSI panel as LVDS
/// ([`D3DKMDT_VOT_LVDS`](https://learn.microsoft.com/windows-hardware/drivers/ddi/d3dkmdt/ne-d3dkmdt-_d3dkmdt_video_output_technology)).
pub(crate) const INTERNAL: i32 = i32::MIN;
pub(crate) const DISPLAYPORT_EMBEDDED: i32 = 11;
pub(crate) const UDI_EMBEDDED: i32 = 13;
pub(crate) const LVDS: i32 = 6;

/// Whether an output of `technology` is a panel built into the device.
/// Chromium's `IsInternalOutputTechnology` leaves LVDS out, so `isInternal`
/// is true here and false in Chrome for a device whose panel is LVDS or
/// DSI; neither connects a monitor on a cable, and the Linux host counts
/// both.
pub(crate) fn built_in(technology: i32) -> bool {
    matches!(
        technology,
        INTERNAL | DISPLAYPORT_EMBEDDED | UDI_EMBEDDED | LVDS
    )
}

/// A run with no window paints a buffer, and that buffer is its one display
/// (LLP 1178 D6), as on the Linux host: its viewport at its scale, and the
/// pointer the run draws.
pub(crate) fn install_headless() {
    let screen = exact_painted::desktop::FullScreen::headless();
    exact_painted::desktop::install_displays(std::rc::Rc::new(screen));
}

/// The session window's client area: its top left in physical pixels, and
/// the window's DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Client {
    pub(crate) origin: [i32; 2],
    pub(crate) dpi: u32,
}

/// A rectangle in whole CSS pixels: x, y, width, height.
type Css = [i32; 4];

/// The side of its parent a monitor lies on.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// Where a monitor lies against the one it touches: the side, and how far
/// along it, from the parent's start or (`from_end`) back from its end.
struct Placement {
    parent: usize,
    child: usize,
    side: Side,
    from_end: bool,
    offset: i32,
}

/// `rect` over `scale`, grown to whole pixels: `gfx::ScaleToEnclosingRect`
/// of `1.0f / scale`, which multiplies. Dividing by the scale is not the
/// same `f32` arithmetic: 1680 pixels at 1.75 are 961 this way, as in
/// Chrome, and 960 that way.
fn enclosing([left, top, right, bottom]: [i32; 4], scale: f32) -> Css {
    if scale == 1. {
        return [left, top, right - left, bottom - top];
    }
    let by = 1. / scale;
    let floor = |v: i32| (v as f32 * by).floor() as i32;
    let ceil = |v: i32| (v as f32 * by).ceil() as i32;
    let (x, y) = (floor(left), floor(top));
    let r = if right == left { x } else { ceil(right) };
    let b = if bottom == top { y } else { ceil(bottom) };
    [x, y, r - x, b - y]
}

/// The overlap of two physical rectangles on each axis, as its start and
/// end; they touch where an axis's start equals its end.
fn overlap(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

/// Whether two monitors share an edge or a corner (`DisplayInfosTouch`).
fn touch(a: [i32; 4], b: [i32; 4]) -> bool {
    let [left, top, right, bottom] = overlap(a, b);
    (left == right && a[1] <= b[3] && b[1] <= a[3])
        || (top == bottom && a[0] <= b[2] && b[0] <= a[2])
}

/// `CalculateDisplayPosition`, for two monitors that touch.
fn side(parent: [i32; 4], current: [i32; 4]) -> Side {
    let [left, top, right, bottom] = overlap(parent, current);
    if left == right && top == bottom {
        // They touch at a corner.
        if parent[3] == top {
            Side::Bottom
        } else if parent[0] == left {
            Side::Left
        } else {
            Side::Top
        }
    } else if left == right && parent[1] <= current[3] && current[1] <= parent[3] {
        if parent[0] == left {
            Side::Left
        } else {
            Side::Right
        }
    } else if parent[1] == top {
        Side::Top
    } else {
        Side::Bottom
    }
}

/// `offset` of `length` physical pixels, as the same share of the length
/// over `scale` (`ScaleOffset`).
fn scale_offset(length: i32, scale: f32, offset: i32) -> i32 {
    let scaled = length as f32 / scale;
    let share = offset as f32 / length as f32;
    (scaled * share).floor() as i32
}

/// `CalculateDisplayPlacement`.
fn placement(monitors: &[Monitor], parent: usize, child: usize) -> Placement {
    let (p, c) = (&monitors[parent], &monitors[child]);
    let side = side(p.rect, c.rect);
    let axis = usize::from(matches!(side, Side::Left | Side::Right));
    // Along the shared edge, from the parent's start.
    let start = p.rect[axis];
    let parent_end = p.rect[axis + 2] - start;
    let (begin, end) = (c.rect[axis] - start, c.rect[axis + 2] - start);
    let (from_end, offset) = if parent_end == end && begin != 0 {
        (true, 0)
    } else if (0..=parent_end).contains(&begin) {
        (false, scale_offset(parent_end, p.scale, begin))
    } else if (0..=parent_end).contains(&end) {
        (true, scale_offset(parent_end, p.scale, parent_end - end))
    } else {
        (false, scale_offset(end - begin, c.scale, begin))
    };
    Placement {
        parent,
        child,
        side,
        from_end,
        offset,
    }
}

/// `DisplayLayout::ApplyDisplayPlacement`: the child's origin from its
/// parent's laid-out rectangle.
fn place(p: &Placement, rects: &mut [Css]) {
    let (parent, target) = (rects[p.parent], rects[p.child]);
    let along = if matches!(p.side, Side::Top | Side::Bottom) {
        2
    } else {
        3
    };
    let mut offset = p.offset;
    if p.from_end {
        offset = parent[along] - offset - target[along];
    }
    let offset = offset.min(parent[along]).max(-target[along]);
    let [x, y] = match p.side {
        Side::Top => [parent[0] + offset, parent[1] - target[3]],
        Side::Right => [parent[0] + parent[2], parent[1] + offset],
        Side::Bottom => [parent[0] + offset, parent[1] + parent[3]],
        Side::Left => [parent[0] - target[2], parent[1] + offset],
    };
    (rects[p.child][0], rects[p.child][1]) = (x, y);
}

/// `DeIntersectDisplays`: each monitor, nearest the primary first, is moved
/// off those before it by the shorter of the two moves that clear it.
fn separate(primary: usize, parents: &[Option<usize>], rects: &mut [Css]) {
    let depth = |mut i: usize| {
        let mut depth = 0;
        while i != primary && depth < 100 {
            match parents[i] {
                Some(parent) => i = parent,
                None => return 100,
            }
            depth += 1;
        }
        depth
    };
    let distance = |r: Css| i64::from(r[0]).pow(2) + i64::from(r[1]).pow(2);
    let mut order: Vec<usize> = (0..rects.len()).collect();
    order.sort_by_key(|&i| (depth(i), distance(rects[i]), i));
    for at in 1..order.len() {
        let target = order[at];
        for &source in &order[..at] {
            let (s, t) = (rects[source], rects[target]);
            let width = (s[0] + s[2]).min(t[0] + t[2]) - s[0].max(t[0]);
            let height = (s[1] + s[3]).min(t[1] + t[3]) - s[1].max(t[1]);
            if width <= 0 || height <= 0 {
                continue;
            }
            let clear = |axis: usize| {
                if t[axis] >= 0 {
                    s[axis] + s[axis + 2] - t[axis]
                } else {
                    s[axis] - (t[axis] + t[axis + 2])
                }
            };
            let (dx, dy) = (clear(0), clear(1));
            if dx.abs() <= dy.abs() {
                rects[target][0] += dx;
            } else {
                rects[target][1] += dy;
            }
        }
    }
}

/// Each monitor's rectangle and work area in the web's space
/// (`DisplayInfosToScreenWinDisplays`). A monitor that touches no other
/// keeps its own rectangle over its scale.
fn layout(monitors: &[Monitor]) -> Vec<(Css, Css)> {
    let mut rects: Vec<Css> = (monitors.iter())
        .map(|m| enclosing(m.rect, m.scale))
        .collect();
    let works: Vec<Css> = (monitors.iter())
        .map(|m| enclosing(m.work, m.scale))
        .collect();
    let insets: Vec<[i32; 2]> = (rects.iter().zip(&works))
        .map(|(r, w)| [w[0] - r[0], w[1] - r[1]])
        .collect();
    if let Some(primary) = monitors.iter().position(|m| m.primary) {
        let mut parents = vec![None; monitors.len()];
        let mut remaining: Vec<usize> = (0..monitors.len()).filter(|&i| i != primary).collect();
        let mut available = vec![primary];
        while let Some(parent) = available.pop() {
            let (touching, apart): (Vec<usize>, Vec<usize>) = (remaining.iter().copied())
                .partition(|&i| touch(monitors[parent].rect, monitors[i].rect));
            remaining = apart;
            for child in touching {
                // A parent is placed before its children are found.
                place(&placement(monitors, parent, child), &mut rects);
                parents[child] = Some(parent);
                available.push(child);
            }
        }
        separate(primary, &parents, &mut rects);
    }
    (rects.into_iter().zip(works).zip(insets))
        .map(|((r, w), [dx, dy])| (r, [r[0] + dx, r[1] + dy, w[2], w[3]]))
        .collect()
}

/// The monitors as the Window Management API's screens, in its order (by
/// `left`, then `top`), with each of `picks`, an index into `monitors`,
/// as that monitor's place in the order.
fn arrange<const N: usize>(
    monitors: &[Monitor],
    placed: &[(Css, Css)],
    client: Option<Client>,
    picks: [usize; N],
) -> (Vec<Screen>, [usize; N]) {
    let mut order: Vec<usize> = (0..monitors.len()).collect();
    order.sort_by_key(|&i| (placed[i].0[0], placed[i].0[1]));
    let screens = (order.iter())
        .map(|&i| {
            let (m, (rect, work)) = (&monitors[i], placed[i]);
            let [left, top, right, bottom] = m.rect.map(f64::from);
            Screen {
                rect: rect.map(f64::from),
                avail: work.map(f64::from),
                scale: f64::from(m.scale),
                primary: m.primary,
                internal: m.internal,
                label: m.label.clone(),
                client: client.map(|c| {
                    let scale = f64::from(c.dpi) / 96.;
                    let [x, y] = c.origin.map(f64::from);
                    [
                        (left - x) / scale,
                        (top - y) / scale,
                        (right - left) / scale,
                        (bottom - top) / scale,
                    ]
                }),
            }
        })
        .collect();
    let ordered = |pick: usize| order.iter().position(|&i| i == pick).unwrap_or(0);
    (screens, picks.map(ordered))
}

/// The monitor `currentScreen` names: the window's, else the primary.
fn current_of(monitors: &[Monitor], current: Option<usize>) -> usize {
    current
        .or(monitors.iter().position(|m| m.primary))
        .unwrap_or(0)
}

/// `ScreenDetails`: `current` is the monitor holding most of the session's
/// window, by its index, or `None` with no window.
pub(crate) fn screens(
    monitors: &[Monitor],
    current: Option<usize>,
    client: Option<Client>,
) -> Screens {
    let picks = [current_of(monitors, current)];
    let (screens, [current]) = arrange(monitors, &layout(monitors), client, picks);
    Screens { screens, current }
}

/// `PointerPosition` for a pointer at the physical position `at`, on the
/// monitor `on`.
pub(crate) fn pointer(
    monitors: &[Monitor],
    current: Option<usize>,
    on: usize,
    at: [i32; 2],
    client: Option<Client>,
) -> Option<Pointer> {
    let holder = monitors.get(on)?;
    let placed = layout(monitors);
    // `ScreenToDIPPoint`: from the holder's physical origin to its laid-out one.
    let scale = f64::from(holder.scale);
    let origin = placed[on].0;
    let from =
        |axis: usize| f64::from(origin[axis]) + f64::from(at[axis] - holder.rect[axis]) / scale;
    let picks = [current_of(monitors, current), on];
    let (screens, [current, on]) = arrange(monitors, &placed, client, picks);
    Some(Pointer {
        at: [from(0), from(1)],
        client: client.map(|c| {
            let scale = f64::from(c.dpi) / 96.;
            [0, 1].map(|axis| f64::from(at[axis] - c.origin[axis]) / scale)
        }),
        screens: Screens { screens, current },
        on,
    })
}

#[cfg(test)]
mod tests;
