//! Region sizing and placement for painted invoker popovers (LLP 1021 D2).
use super::BlockTactic;
use crate::{BlockAxis, BlockMeasure, Dimension, Direction, Env, HostBlock, StyleId, StyleProps};

type Rect = (f32, f32, f32, f32);
const FALLBACKS: &str = "flip-block, flip-inline, flip-block flip-inline";

/// The selected option belongs to one opening and survives anchor movement and resize.
#[derive(Default)]
pub struct History {
    choice: usize,
    inputs: Option<Options>,
    scroll: Option<(f32, f32)>,
}
#[derive(PartialEq)]
struct Options {
    area: String,
    fallbacks: String,
    rtl: bool,
    margins: [Dimension; 4],
    sizing: [Dimension; 6],
    size: (f32, f32),
}

/// A border box's origin, and the region and tactic to retain for its final layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The border box's viewport origin.
    pub origin: (f32, f32),
    /// The selected region, or `None` to retain the implied viewport block.
    pub block: Option<HostBlock>,
}
#[derive(Clone, Copy)]
struct Attempt {
    placement: Placement,
    outer: Rect,
    sides: (i8, i8),
    region: Rect,
}

/// Measure each option in its inset-modified containing block and keep the first
/// whose margin box fits. The current fitting option survives movement; if none
/// fits, its overflow alignment wins. `measure` is `Kernel::measure_block` on the
/// popover after the page's layout. The caller retains the returned block and lays
/// out again before using the origin, so descendants and percentages agree.
/// `scroll` sums the offsets between the invoker and its viewport; the selected
/// option keeps its sizing basis while that scroll moves it.
pub fn place<E>(
    style: &StyleProps,
    env: &Env,
    anchor: Rect,
    viewport: (f32, f32),
    scroll: (f32, f32),
    history: &mut History,
    mut measure: impl FnMut(HostBlock) -> Result<BlockMeasure, E>,
) -> Result<Placement, E> {
    let rtl = style.direction == Direction::Rtl;
    let said = style.mask.union(style.written);
    let area = if said.has(StyleId::PositionArea) {
        style.position_area.name()
    } else if rtl {
        "bottom span-left"
    } else {
        "bottom span-right"
    };
    let fallbacks = if said.has(StyleId::PositionTryFallbacks) {
        style.position_try_fallbacks.css()
    } else if said.has(StyleId::PositionArea) {
        "none".into()
    } else {
        FALLBACKS.into()
    };
    let block = |width, height, tactic| HostBlock {
        width: BlockAxis::Region(width),
        height: BlockAxis::Region(height),
        tactic,
    };
    // Use the whole block's size for invalidation, independent of the chosen
    // region and tactic (which may have exchanged width and height).
    let whole = if area == "none" {
        HostBlock {
            width: BlockAxis::Fit(viewport.0),
            height: BlockAxis::Fit(viewport.1),
            tactic: BlockTactic::default(),
        }
    } else {
        block(viewport.0, viewport.1, BlockTactic::default())
    };
    let measured = measure(whole)?;
    if area == "none" {
        *history = History::default();
        let m = measured.margin;
        let outer = (
            measured.border.0 + m[1] + m[3],
            measured.border.1 + m[0] + m[2],
        );
        return Ok(Placement {
            origin: (
                anchor.0.min(viewport.0 - outer.0).max(0.) + m[3],
                (anchor.1 + anchor.3).min(viewport.1 - outer.1).max(0.) + m[0],
            ),
            block: None,
        });
    }
    let size = measured.border;
    let options = Options {
        area: area.into(),
        fallbacks,
        rtl,
        margins: [
            style.margin_top.clone(),
            style.margin_right.clone(),
            style.margin_bottom.clone(),
            style.margin_left.clone(),
        ]
        .map(|d| d.resolve(env)),
        sizing: [
            style.width.clone(),
            style.height.clone(),
            style.min_width.clone(),
            style.min_height.clone(),
            style.max_width.clone(),
            style.max_height.clone(),
        ]
        .map(|d| d.resolve(env)),
        size,
    };
    if history.inputs.as_ref() != Some(&options) {
        history.choice = 0;
        history.scroll = None;
    }
    let tries: Vec<_> = std::iter::once("none")
        .chain(
            options
                .fallbacks
                .split(',')
                .map(str::trim)
                .filter(|s| *s != "none"),
        )
        .collect();
    let base = axes(area);
    let mut attempt = |index: usize, anchor: Rect| -> Result<Attempt, E> {
        let (x, y, sides) = flipped(tries[index], base, [0, 1, 2, 3], rtl);
        let r = region((x, y), anchor, viewport);
        let block = block(
            r.2,
            r.3,
            BlockTactic::from_sides(sides).expect("ordered flips"),
        );
        let measured = measure(block)?;
        let (size, m) = (measured.border, measured.margin);
        let outer = (size.0 + m[1] + m[3], size.1 + m[0] + m[2]);
        let corner = (
            at(x, anchor.0, anchor.0 + anchor.2, outer.0, viewport.0),
            at(y, anchor.1, anchor.1 + anchor.3, outer.1, viewport.1),
        );
        Ok(Attempt {
            placement: Placement {
                origin: (corner.0 + m[3], corner.1 + m[0]),
                block: Some(block),
            },
            outer: (corner.0, corner.1, outer.0, outer.1),
            sides: (x, y),
            region: r,
        })
    };
    // CSS remembers the scroll at selection (§5.2). Scrolling translates that
    // option's box; it does not change its percentage or fit-content basis.
    let remembered = *history.scroll.get_or_insert(scroll);
    let shift = (scroll.0 - remembered.0, scroll.1 - remembered.1);
    let then = (anchor.0 + shift.0, anchor.1 + shift.1, anchor.2, anchor.3);
    let current = attempt(history.choice, then)?;
    let mut moved = current;
    moved.placement.origin.0 -= shift.0;
    moved.placement.origin.1 -= shift.1;
    moved.outer.0 -= shift.0;
    moved.outer.1 -= shift.1;
    moved.region = region(moved.sides, anchor, viewport);
    let placement = if fits(&moved, viewport) {
        moved.placement
    } else {
        let mut selected = None;
        for index in (0..tries.len()).filter(|i| *i != history.choice) {
            let candidate = attempt(index, anchor)?;
            if fits(&candidate, viewport) {
                selected = Some((index, candidate.placement));
                break;
            }
        }
        if let Some((index, placement)) = selected {
            history.choice = index;
            history.scroll = Some(scroll);
            placement
        } else {
            let origin = overflowed(
                current.outer,
                current.placement.origin,
                current.sides,
                then,
                rtl,
                viewport,
            );
            Placement {
                origin: (origin.0 - shift.0, origin.1 - shift.1),
                ..current.placement
            }
        }
    };
    history.inputs = Some(options);
    Ok(placement)
}

/// Place a box that has no host block from the size its ordinary layout gave it.
pub fn origin(
    style: &StyleProps,
    env: &Env,
    anchor: Rect,
    size: (f32, f32),
    viewport: (f32, f32),
    history: &mut History,
) -> (f32, f32) {
    place(style, env, anchor, viewport, (0., 0.), history, |block| {
        let margins = [
            style.margin_top.clone(),
            style.margin_right.clone(),
            style.margin_bottom.clone(),
            style.margin_left.clone(),
        ]
        .map(|d| match d.resolve(env) {
            Dimension::Points(n) => n,
            _ => 0.,
        });
        Ok::<_, std::convert::Infallible>(BlockMeasure {
            border: size,
            used: size,
            margin: block.tactic.0.map(|i| margins[usize::from(i)]),
        })
    })
    .unwrap()
    .origin
}

fn band(side: i8, low: f32, high: f32, end: f32) -> (f32, f32) {
    let (min, max) = (0f32.min(low), end.max(high));
    match side {
        -2 => (min, low),
        -1 => (min, high),
        1 => (low, max),
        2 => (high, max),
        -3 | 3 => (low, high),
        _ => (min, max),
    }
}
fn region(sides: (i8, i8), anchor: Rect, viewport: (f32, f32)) -> Rect {
    let x = band(sides.0, anchor.0, anchor.0 + anchor.2, viewport.0);
    let y = band(sides.1, anchor.1, anchor.1 + anchor.3, viewport.1);
    (x.0, y.0, (x.1 - x.0).max(0.), (y.1 - y.0).max(0.))
}
fn fits(a: &Attempt, viewport: (f32, f32)) -> bool {
    let axis = |side: i8, low: f32, length: f32, start: f32, room: f32, end: f32| {
        // anchor-center may overflow the anchor's cell inside the viewport.
        let (start, end) = if side.abs() == 3 {
            (0., end)
        } else {
            (start, start + room)
        };
        low >= start - 0.01 && low + length <= end + 0.01
    };
    axis(
        a.sides.0, a.outer.0, a.outer.2, a.region.0, a.region.2, viewport.0,
    ) && axis(
        a.sides.1, a.outer.1, a.outer.3, a.region.1, a.region.3, viewport.1,
    )
}

fn axes(area: &str) -> (i8, i8) {
    if area == "none" {
        return (1, 2);
    }
    if area == "center" {
        return (3, 3);
    }
    let (mut x, mut y) = (0, 0);
    for word in area.split_ascii_whitespace() {
        match word {
            "left" => x = -2,
            "right" => x = 2,
            "top" => y = -2,
            "bottom" => y = 2,
            "span-left" => x = -1,
            "span-right" => x = 1,
            "span-top" => y = -1,
            "span-bottom" => y = 1,
            _ => {}
        }
    }
    (x, y)
}

fn flipped<T: Copy>(
    option: &str,
    (mut x, mut y): (i8, i8),
    mut m: [T; 4],
    rtl: bool,
) -> (i8, i8, [T; 4]) {
    for word in option.split_ascii_whitespace() {
        match word {
            "flip-block" => {
                y = -y;
                m.swap(0, 2);
            }
            "flip-inline" => {
                x = -x;
                m.swap(1, 3);
            }
            "flip-start" if rtl => {
                (x, y) = (-y, -x);
                m = [m[1], m[0], m[3], m[2]];
            }
            "flip-start" => {
                std::mem::swap(&mut x, &mut y);
                m = [m[3], m[2], m[1], m[0]];
            }
            _ => {}
        }
    }
    (x, y, m)
}

fn at(side: i8, low: f32, high: f32, length: f32, end: f32) -> f32 {
    match side {
        -2 => low - length,
        -1 => high - length,
        1 => low,
        2 => high,
        _ => ((low + high - length) / 2.).min(end - length).max(0.),
    }
}

fn overflowed(
    rect: Rect,
    point: (f32, f32),
    sides: (i8, i8),
    anchor: Rect,
    rtl: bool,
    viewport: (f32, f32),
) -> (f32, f32) {
    fn limit(side: i8, low: f32, high: f32, end: f32) -> (f32, f32) {
        let area = band(side, low, high, end);
        (0f32.min(area.0), end.max(area.1))
    }
    fn shifted(at: f32, length: f32, range: (f32, f32), start_high: bool) -> f32 {
        if start_high {
            at.max(range.0).min(range.1 - length)
        } else {
            at.min(range.1 - length).max(range.0)
        }
    }
    let x = shifted(
        rect.0,
        rect.2,
        limit(sides.0, anchor.0, anchor.0 + anchor.2, viewport.0),
        rtl,
    );
    let y = shifted(
        rect.1,
        rect.3,
        limit(sides.1, anchor.1, anchor.1 + anchor.3, viewport.1),
        false,
    );
    (point.0 + x - rect.0, point.1 + y - rect.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    type Margins = [f32; 4];
    type Rect4 = Rect;

    fn origin(
        style: &StyleProps,
        env: &Env,
        anchor: Rect,
        size: (f32, f32),
        viewport: (f32, f32),
        history: &mut History,
    ) -> (f32, f32) {
        super::place(style, env, anchor, viewport, (0., 0.), history, |block| {
            let margins = [
                style.margin_top.clone(),
                style.margin_right.clone(),
                style.margin_bottom.clone(),
                style.margin_left.clone(),
            ]
            .map(|d| d.at_basis(env, block.width.length()));
            let margin = block.tactic.0.map(|i| margins[usize::from(i)]);
            Ok::<_, ()>(BlockMeasure {
                border: size,
                margin,
                used: size,
            })
        })
        .unwrap()
        .origin
    }

    fn place(
        area: &str,
        fallbacks: &str,
        rtl: bool,
        margins: Margins,
        anchor: Rect4,
        size: (f32, f32),
        viewport: (f32, f32),
    ) -> (f32, f32) {
        let mut style = StyleProps::default();
        style
            .set_dynamic(StyleId::PositionArea, &crate::StyleValue::Text(area.into()))
            .unwrap();
        style
            .set_dynamic(
                StyleId::PositionTryFallbacks,
                &crate::StyleValue::Text(fallbacks.into()),
            )
            .unwrap();
        style.direction = if rtl { Direction::Rtl } else { Direction::Ltr };
        [
            style.margin_top,
            style.margin_right,
            style.margin_bottom,
            style.margin_left,
        ] = margins.map(Dimension::Points);
        origin(
            &style,
            &Env::default(),
            anchor,
            size,
            viewport,
            &mut History::default(),
        )
    }

    #[test]
    fn last_successful_option_survives_more_room_but_resets_for_size_and_style() {
        let mut history = History::default();
        let env = Env::default();
        let mut style = StyleProps::default();
        let anchor = (100., 170., 40., 20.);
        assert_eq!(
            origin(&style, &env, anchor, (80., 50.), (300., 200.), &mut history),
            (100., 120.)
        );
        assert_eq!(history.choice, 1);
        assert_eq!(
            origin(
                &style,
                &env,
                (100., 20., 40., 20.),
                (80., 50.),
                (300., 60.),
                &mut history
            ),
            (100., 0.)
        );
        assert_eq!(history.choice, 1);
        assert_eq!(
            origin(&style, &env, anchor, (80., 50.), (300., 300.), &mut history),
            (100., 120.)
        );
        assert_eq!(
            origin(&style, &env, anchor, (80., 60.), (300., 300.), &mut history),
            (100., 190.)
        );
        assert_eq!(history.choice, 0);
        assert_eq!(
            origin(&style, &env, anchor, (80., 60.), (300., 200.), &mut history),
            (100., 110.)
        );
        style.margin_top = Dimension::Points(4.);
        assert_eq!(
            origin(&style, &env, anchor, (80., 60.), (300., 300.), &mut history),
            (100., 194.)
        );
        assert_eq!(history.choice, 0);
    }

    #[test]
    fn scrolling_keeps_the_sizing_region_until_another_option_wins() {
        let mut style = StyleProps::default();
        style
            .set_dynamic(
                StyleId::PositionArea,
                &crate::StyleValue::Text("bottom span-right".into()),
            )
            .unwrap();
        style
            .set_dynamic(
                StyleId::PositionTryFallbacks,
                &crate::StyleValue::Text("flip-block".into()),
            )
            .unwrap();
        let env = Env::default();
        let mut history = History::default();
        let place = |anchor, scroll, history: &mut History| {
            super::place(
                &style,
                &env,
                anchor,
                (400., 600.),
                scroll,
                history,
                |block| {
                    let size = (block.width.length() / 2., block.height.length() / 2.);
                    Ok::<_, ()>(BlockMeasure {
                        border: size,
                        used: size,
                        margin: [0.; 4],
                    })
                },
            )
            .unwrap()
        };
        let first = place((100., 100., 40., 20.), (0., 0.), &mut history);
        assert_eq!(first.origin, (100., 120.));
        assert_eq!(first.block.unwrap().height, BlockAxis::Region(480.));
        let scrolled = place((100., 40., 40., 20.), (0., 60.), &mut history);
        assert_eq!(scrolled.origin, (100., 60.));
        assert_eq!(scrolled.block, first.block);
        let flipped = place((100., 500., 40., 20.), (0., -400.), &mut history);
        assert_eq!(history.choice, 1);
        assert_eq!(flipped.origin, (100., 250.));
        assert_eq!(flipped.block.unwrap().height, BlockAxis::Region(500.));
        let scrolled = place((100., 440., 40., 20.), (0., -340.), &mut history);
        assert_eq!(scrolled.origin, (100., 190.));
        assert_eq!(scrolled.block, flipped.block);
    }

    #[test]
    fn defaults_authored_areas_and_margins() {
        let env = Env::default();
        let mut s = StyleProps::default().with_rows(|rows| {
            rows.margin_top = Dimension::Points(4.);
        });
        assert_eq!(
            origin(
                &s,
                &env,
                (100., 80., 40., 20.),
                (80., 50.),
                (300., 200.),
                &mut History::default()
            ),
            (100., 104.)
        );
        s.direction = Direction::Rtl;
        assert_eq!(
            origin(
                &s,
                &env,
                (100., 80., 40., 20.),
                (80., 50.),
                (300., 200.),
                &mut History::default()
            ),
            (60., 104.)
        );
        s.position_area = crate::PositionArea::TopSpanLeft;
        s.mask.set(StyleId::PositionArea);
        s.margin_bottom = Dimension::Points(6.);
        assert_eq!(
            origin(
                &s,
                &env,
                (100., 80., 40., 20.),
                (80., 50.),
                (300., 200.),
                &mut History::default()
            ),
            (60., 24.)
        );
    }

    #[test]
    fn attempts_flip_the_base_in_order_and_keep_margins() {
        let bounds = (300., 200.);
        let size = (80., 50.);
        assert_eq!(
            place(
                "bottom span-right",
                FALLBACKS,
                false,
                [4., 3., 6., 2.],
                (250., 170., 40., 20.),
                size,
                bounds
            ),
            (208., 116.)
        );
        assert_eq!(
            flipped("flip-start flip-block", (1, 2), [1., 2., 3., 4.], false),
            (2, -1, [2., 3., 4., 1.])
        );
        assert_eq!(
            flipped("flip-block flip-start", (1, 2), [1., 2., 3., 4.], false),
            (-2, 1, [4., 1., 2., 3.])
        );
        assert_eq!(
            flipped("flip-start", (1, 2), [1., 2., 3., 4.], true),
            (-2, -1, [2., 1., 4., 3.])
        );
        assert_eq!(
            place(
                "center",
                "none",
                false,
                [0.; 4],
                (140., 90., 20., 20.),
                size,
                bounds
            ),
            (110., 75.)
        );
        assert_eq!(
            place(
                "right",
                "none",
                false,
                [0.; 4],
                (280., 80., 20., 20.),
                size,
                bounds
            ),
            (220., 65.)
        );
        assert_eq!(
            place(
                "center",
                "none",
                true,
                [0.; 4],
                (140., 90., 20., 20.),
                (400., 250.),
                bounds
            ),
            (-100., 0.)
        );
    }
}
