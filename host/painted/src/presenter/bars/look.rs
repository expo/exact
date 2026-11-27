//! Material 3's small top app bar and its navigation bar, measured and
//! painted (LLP 1115.001 D11, D1.3). Painted, not built of framework
//! widgets: each bar is a view of the reader's, and what is in it is drawn
//! here from the reference's own metrics, the Compose Material 3 library's
//! (1.4: `TopAppBar` with `TopAppBarSmallTokens`, `NavigationBar` and
//! `NavigationBarItem` with `NavigationBarTokens`), using device roles wherever the bar leaves its colours unsaid.
//!
//! Lengths are dp, which are this host's points. Type is sp, a point times
//! the person's font scale, taken linearly, as the top layer's is
//! (`top/look.rs`).
//!
//! **The top app bar** is 64 tall under the status bar, whose area it
//! fills with its own colour. The defaults are `surface` and `surfaceContainer` once the
//! route's content has scrolled under it (Material's "on scroll" colour).
//! It does not collapse. The Up arrow is a 24 icon in a 48 target, 4 from
//! the edge; the title is title-large, 16 from the edge, or 4 after what
//! leads it (56 from the edge after the arrow); the header's buttons are at
//! the trailing edge, 4 from it: an icon button where the button has a
//! symbol, else a text button. Its numbers are the `top-` rows.
//!
//! **The navigation bar** is 80 tall over the gesture bar, whose area it
//! fills with its own colour, by default `surfaceContainer`. Its destinations share
//! its width, 8 apart: a 24 symbol with its label under it (label-medium),
//! and behind the selected one's symbol the active indicator, a 56 by 32
//! pill in `secondaryContainer`. The selected destination's symbol is
//! `onSecondaryContainer` and its label `secondary`, the others'
//! `onSurfaceVariant`, and a disabled one's that at 38 %
//! (`NavigationBarItemDefaults.colors`). Its numbers are the schema's
//! Material column (the `nav-` rows; `host/android/reference/rows.json`
//! names each one's symbol).
use super::badge::DrawnBadge;
pub use super::badge::{Badge, BadgePicture};
use crate::paint::{paint_symbol, Backend, Rect4, Shape};
use crate::presenter::palette::{material, Material};
use crate::presenter::top::look::{
    directed_paragraph, label_x, paragraph, text_rect, Role, Type, LABEL_LARGE, STATE_LAYER,
};
use crate::text::{Paragraph, RunPaint, TextEngine};
use exact_kernel::generated::metric;
use exact_kernel::style::metrics::{self, Column};
use exact_kernel::{Direction, ViewId};
use material_colors::{color::Rgb, dynamic_color::DynamicScheme, scheme::variant::SchemeTonalSpot};
use std::rc::Rc;
use tiny_skia::Transform;

pub(in crate::presenter) mod ink;
use ink::Accent;

const TITLE_LARGE: Type = Type(22.0, 28.0, 400, 0.0);
const LABEL_MEDIUM: Type = Type(12.0, 16.0, 500, 0.5);
const LABEL_SMALL: Type = Type(11.0, 16.0, 500, 0.5);

/// The top app bar's default height, under the status bar.
pub fn top_height() -> f32 {
    dp(metric::TOP_HEIGHT)
}

/// The top app bar's content height under the status bar at `font_scale`
/// (LLP 1115.001 D11): 64 dp (`AppBarSmallTokens.ContainerHeight`), or the
/// scaled title (and optional subtitle) line heights when larger.
pub fn top_content_height(font_scale: f32, subtitled: bool) -> f32 {
    top_height().max(top_minimum(font_scale, subtitled))
}

pub(super) fn top_minimum(font_scale: f32, subtitled: bool) -> f32 {
    let lines = TITLE_LARGE.1 + if subtitled { LABEL_MEDIUM.1 } else { 0.0 };
    ICON_BUTTON.max(lines * font_scale)
}

/// A badge with text: its least size, its text's inset, and where it sits
/// on its anchor (`BadgeTokens.LargeSize`, `BadgeWithContentHorizontalPadding`,
/// `BadgeWithContentHorizontalOffset`, `BadgeWithContentVerticalOffset`).
const BADGE: f32 = 16.0;
const BADGE_PADDING: f32 = 4.0;
const BADGE_OFFSET: (f32, f32) = (12.0, 14.0);

/// An icon button: a 48 target (the minimum interactive size), its state
/// layer a 40 circle.
const ICON_BUTTON: f32 = 48.0;
const ICON_LAYER: f32 = 40.0;
/// A text button: at least 58 by 40, its label 12 from its sides.
const TEXT_BUTTON: (f32, f32) = (58.0, 40.0);
const TEXT_BUTTON_PADDING: f32 = 12.0;

/// A length of the Material look, dp.
fn dp(index: usize) -> f32 {
    metrics::length_in(index, Column::Material, 0.0, 0.0).expect("a Material row")
}

/// The navigation bar's numbers: its height over the gesture bar, where its
/// labels fit; its symbol; the active indicator, and the room it leaves
/// above and under the symbol; between the indicator and the label; and
/// between two destinations.
struct Nav {
    height: f32,
    icon: f32,
    indicator: (f32, f32),
    indicator_padding: f32,
    label_gap: f32,
    item_gap: f32,
}

fn nav() -> Nav {
    let (icon, indicator) = (
        dp(metric::NAV_ICON),
        (
            dp(metric::NAV_INDICATOR_WIDTH),
            dp(metric::NAV_INDICATOR_HEIGHT),
        ),
    );
    Nav {
        height: dp(metric::NAV_HEIGHT),
        icon,
        indicator,
        indicator_padding: (indicator.1 - icon) / 2.0,
        label_gap: dp(metric::NAV_LABEL_GAP),
        item_gap: dp(metric::NAV_ITEM_GAP),
    }
}

/// A Material role the shell told, else Material's baseline (Compose's
/// `lightColorScheme()` and `darkColorScheme()`), where the twelve roles
/// LLP 1115.001 D4 maps have none of their own for it.
fn told(role: Material, dark: bool, light: u32, night: u32) -> [u8; 4] {
    material(role, dark).unwrap_or_else(|| {
        let [_, r, g, b] = if dark { night } else { light }.to_be_bytes();
        [r, g, b, 255]
    })
}

fn disabled(mut ink: [u8; 4]) -> [u8; 4] {
    ink[3] = (f32::from(ink[3]) * 0.38).round() as u8;
    ink
}

/// A header's button as the top bar shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    /// The authored `button`.
    pub node: ViewId,
    /// What it says: its title, else its label.
    pub title: String,
    /// The title's computed text direction.
    pub direction: Direction,
    /// Its symbol role, which makes it an icon button.
    pub symbol: Option<String>,
    /// The symbol image's computed direction.
    pub symbol_direction: Direction,
    /// Its constrained badge face (filled box holding a picture, text or symbol).
    pub badge: Option<Badge>,
    /// Not `disabled`.
    pub enabled: bool,
    /// `destructive`.
    pub destructive: bool,
    /// The colour its author wrote on it, as it resolves now.
    pub tint: Option<[u8; 4]>,
}

/// The top app bar, as said.
#[derive(Debug, Clone, PartialEq)]
pub struct Top {
    /// The route whose header it is.
    pub route: ViewId,
    /// The heading: none for a header of buttons alone, whose bar has no title.
    pub heading: Option<ViewId>,
    /// The header's computed direction.
    pub direction: Direction,
    /// The title.
    pub title: String,
    /// The heading's computed text direction, independent of the header's placement.
    pub title_direction: Direction,
    /// The supporting subtitle node, when the header supplies one (LLP 1115.001 D11).
    pub subtitle_node: Option<ViewId>,
    /// The supporting subtitle text, empty when absent.
    pub subtitle: String,
    /// The subtitle's computed text direction, independent of the header's placement.
    pub subtitle_direction: Direction,
    /// Whether it has the Up arrow: the route is pushed.
    pub up: bool,
    /// The buttons before the heading, and those after it.
    pub leading: Vec<Button>,
    /// See `leading`.
    pub trailing: Vec<Button>,
    /// Whether the route's content has scrolled under it.
    pub scrolled: bool,
}

impl Top {
    fn has_subtitle(&self) -> bool {
        self.heading.is_some()
            && !self.title.is_empty()
            && self.subtitle_node.is_some()
            && !self.subtitle.is_empty()
    }
}

/// One destination of the navigation bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    /// The authored tab.
    pub node: ViewId,
    /// Its label.
    pub label: String,
    /// The label's computed text direction.
    pub direction: Direction,
    /// Its symbol role.
    pub symbol: Option<String>,
    /// The symbol image's computed direction.
    pub symbol_direction: Direction,
    /// `aria-selected`.
    pub selected: bool,
    /// Not `disabled`.
    pub enabled: bool,
    /// Its badge (LLP 1075.003 §9.9, `PressFace.badge`).
    pub badge: Option<String>,
}

/// The navigation bar, as said.
#[derive(Debug, Clone, PartialEq)]
pub struct Bottom {
    /// The authored `tablist`.
    pub tablist: ViewId,
    /// The tablist's computed direction.
    pub direction: Direction,
    /// Its destinations, in order.
    pub tabs: Vec<Tab>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BarFill {
    pub paint: [u8; 4],
    /// The fill over the window, as seen through the system band.
    pub ground: [u8; 4],
}

/// CSS written on the element the bar replaces (LLP 1115.001 D11).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct BarColors {
    pub container: Option<BarFill>,
    pub ink: Option<[u8; 4]>,
    pub seed: Option<([u8; 3], f32)>,
}

impl BarColors {
    fn accent(self, dark: bool) -> Option<DynamicScheme> {
        let ([r, g, b], contrast) = self.seed?;
        Some(SchemeTonalSpot::new(Rgb::new(r, g, b).into(), dark, Some(f64::from(contrast))).scheme)
    }
}

fn opaque(color: Rgb) -> [u8; 4] {
    [color.red, color.green, color.blue, 255]
}

/// What a bar's look depends on besides what it says.
#[derive(Clone, Copy)]
pub struct Inputs<'a> {
    pub(super) device_inks: &'a ink::DeviceInks,
    pub(super) colors: BarColors,
    /// The scheme is dark.
    pub dark: bool,
    /// The person's font scale.
    pub font_scale: f32,
    /// The window's width, which the bar spans.
    pub width: f32,
    /// The system's bar the bar reaches under and fills with its colour:
    /// the status bar above the top bar, the gesture bar under the
    /// navigation bar.
    pub edge: f32,
    /// The window's safe insets at its left and right (a cutout in
    /// landscape): what is in the bar keeps inside them.
    pub sides: (f32, f32),
}

/// What a touch in a bar is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// The Up arrow: Back, in its order (D6).
    Up,
    /// A header's button, by its authored node.
    Item(ViewId),
    /// A destination, by its authored tab.
    Tab(ViewId),
}

/// A target shown held: its place among the bar's targets, and a finger's
/// ripple on it at an instant and whether motion is removed, or none for a
/// TV remote's focus, which is its state layer.
pub type Lit = (usize, Option<(crate::paint::ripple::Ripple, f64, bool)>);

/// One thing in a bar that can be touched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    /// Its box in the bar, points.
    pub rect: Rect4,
    /// What it is.
    pub what: What,
    /// Whether it takes a press.
    pub enabled: bool,
    layer: (Shape, [u8; 4]),
}

impl Target {
    /// A point of the bar as an offset into the shape its press shows in.
    pub(super) fn held_at(&self, x: f32, y: f32) -> (f32, f32) {
        let (left, top, ..) = self.layer.0.rect;
        (x - left, y - top)
    }
}

pub(super) enum Item {
    Text(Rc<Paragraph>, (f32, f32), [u8; 4], ViewId, Option<Rect4>),
    Fill(Shape, [u8; 4]),
    Symbol(String, Rect4, [u8; 4], Direction),
    Badge(DrawnBadge),
}

fn clip_rect(r: Rect4, clip: Option<Rect4>) -> Rect4 {
    let Some(c) = clip else {
        return r;
    };
    let x0 = r.0.max(c.0);
    let y0 = r.1.max(c.1);
    let x1 = (r.0 + r.2).min(c.0 + c.2);
    let y1 = (r.1 + r.3).min(c.1 + c.3);
    (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Resolve inline positions into physical boxes without reflecting glyphs.
fn mirror(items: &mut [Item], targets: &mut [Target], width: f32) {
    let rect = |r: &mut Rect4| r.0 = width - r.0 - r.2;
    for item in items {
        match item {
            Item::Text(p, at, _, _, clip) => {
                let bounds = text_rect(p, *at);
                at.0 = label_x(p, width - bounds.0, 1.);
                if let Some(c) = clip {
                    rect(c);
                }
            }
            Item::Fill(shape, _) => rect(&mut shape.rect),
            Item::Symbol(_, r, _, _) => rect(r),
            Item::Badge(badge) => badge.mirror(width),
        }
    }
    for target in targets {
        rect(&mut target.rect);
        rect(&mut target.layer.0.rect);
    }
}

/// A bar measured: its size, its colour and what it draws.
pub struct Layout {
    /// Width and height, points: the system's bar it reaches under included.
    pub size: (f32, f32),
    container: [u8; 4],
    pub(super) items: Vec<Item>,
    /// What can be touched, in order.
    pub targets: Vec<Target>,
}

/// One line of `style`, cut with an ellipsis where it is wider than `max`.
fn line(
    engine: &mut TextEngine,
    text: &str,
    style: Type,
    scale: f32,
    max: f32,
    direction: Direction,
) -> Rc<Paragraph> {
    let line = directed_paragraph(engine, text, style, scale, None, direction);
    line.ellipsized(max.max(1.0)).unwrap_or(line)
}

/// The navigation bar's height at a font scale: 80, or what its symbol, its
/// label and the indicator's own padding need where that is more (as
/// `NavigationBarItem` places them).
pub fn nav_height(font_scale: f32) -> f32 {
    let nav = nav();
    let content = nav.icon + nav.indicator_padding + nav.label_gap + LABEL_MEDIUM.1 * font_scale;
    let padding = ((nav.height - content) / 2.0).max(nav.indicator_padding);
    content + 2.0 * padding
}

impl Layout {
    /// Each text it draws, by the node it stands for, with its box in the
    /// bar, points: what an accessibility service reads there.
    pub fn texts(&self) -> Vec<(ViewId, Rect4)> {
        (self.items.iter())
            .filter_map(|item| match item {
                Item::Text(p, at, _, source, clip) => {
                    Some((*source, clip_rect(text_rect(p, *at), *clip)))
                }
                _ => None,
            })
            .collect()
    }

    /// The target at a point of the bar.
    pub fn target_at(&self, x: f32, y: f32) -> Option<usize> {
        self.targets.iter().position(|t| {
            x >= t.rect.0 && y >= t.rect.1 && x < t.rect.0 + t.rect.2 && y < t.rect.1 + t.rect.3
        })
    }

    /// Paint the bar into `backend`, its top left at the origin: its
    /// colour over all of it, the target `lit` (a finger's ripple, at a
    /// moment of it, or a TV remote's focus, its state layer), then what it
    /// says.
    pub fn paint(&self, backend: &mut dyn Backend, engine: &mut TextEngine, lit: Option<Lit>) {
        let ts = Transform::identity();
        let all = Shape::rect((0.0, 0.0, self.size.0, self.size.1));
        backend.fill(&all, self.container, ts);
        // The indicator and authored badge faces are under the state layer,
        // and symbols/labels above it.
        for item in &self.items {
            match item {
                Item::Fill(shape, color) => backend.fill(shape, *color, ts),
                Item::Badge(badge) => badge.paint(backend, engine, ts),
                _ => {}
            }
        }
        if let Some((target, ripple)) = lit.and_then(|(i, r)| Some((self.targets.get(i)?, r))) {
            let (shape, mut tint) = target.layer;
            match ripple {
                Some((ripple, now, still)) => {
                    let wave = ripple.wave(shape.rect, now, still);
                    crate::paint::ripple::fill(backend, &shape, tint, wave, ts);
                }
                None => {
                    tint[3] = (f32::from(tint[3]) * STATE_LAYER).round() as u8;
                    backend.fill(&shape, tint, ts);
                }
            }
        }
        for item in &self.items {
            match item {
                Item::Text(paragraph, at, color, source, clip) => {
                    let paint = [RunPaint {
                        color: *color,
                        source: *source,
                    }];
                    if let Some(c) = clip {
                        backend.push_clip(&Shape::rect(*c), ts);
                    }
                    backend.text(engine, paragraph, &paint, *at, ts);
                    if clip.is_some() {
                        backend.pop_clip();
                    }
                }
                Item::Fill(..) | Item::Badge(..) => {}
                // Material's shapes (D14): the Up arrow is its `back`.
                Item::Symbol(role, rect, color, direction) => paint_symbol(
                    backend,
                    role,
                    *rect,
                    *color,
                    exact_kernel::SymbolSet::Material,
                    400,
                    *direction,
                ),
            }
        }
    }

    /// Material's small top app bar for `top`.
    pub fn top(engine: &mut TextEngine, top: &Top, inputs: &Inputs) -> Layout {
        Self::top_at_height(
            engine,
            top,
            inputs,
            top_content_height(inputs.font_scale, top.has_subtitle()),
        )
    }

    pub(super) fn top_at_height(
        engine: &mut TextEngine,
        top: &Top,
        inputs: &Inputs,
        h: f32,
    ) -> Layout {
        let inputs = if top.direction == Direction::Rtl {
            &Inputs {
                sides: (inputs.sides.1, inputs.sides.0),
                ..*inputs
            }
        } else {
            inputs
        };
        let (dark, scale) = (inputs.dark, inputs.font_scale);
        let has_subtitle = top.has_subtitle();
        let y0 = inputs.edge;
        // What leads and what trails keep `edge` from the bar's sides, and
        // the title has `edge` of its own either side (`TopAppBarLayout`).
        let (edge, icon) = (dp(metric::TOP_EDGE), dp(metric::TOP_ICON));
        let (mut items, mut targets) = (Vec::new(), Vec::new());
        let middle = |size: f32| y0 + (h - size) / 2.0;
        let circle = |x: f32| {
            let at = (x + (ICON_BUTTON - ICON_LAYER) / 2.0, middle(ICON_LAYER));
            Shape::new((at.0, at.1, ICON_LAYER, ICON_LAYER), [ICON_LAYER / 2.0; 4])
        };
        let mut left = inputs.sides.0 + edge;
        let mut right = inputs.width - inputs.sides.1 - edge;
        let base = if top.scrolled {
            Role::SurfaceContainer
        } else {
            Role::Surface
        };
        let container = inputs
            .colors
            .container
            .map_or_else(|| base.color(dark), |f| f.paint);
        let content = inputs.colors.ink;
        let title_ink = content.unwrap_or_else(|| Role::OnSurface.color(dark));
        if top.up {
            let ink = title_ink;
            let at = (left + (ICON_BUTTON - icon) / 2.0, middle(icon), icon, icon);
            items.push(Item::Symbol("back".into(), at, ink, top.direction));
            targets.push(Target {
                rect: (left, middle(ICON_BUTTON), ICON_BUTTON, ICON_BUTTON),
                what: What::Up,
                enabled: true,
                layer: (circle(left), ink),
            });
            left += ICON_BUTTON;
        }
        // A button's box is its 48 target; what it draws sits in the middle.
        let primary = std::cell::LazyCell::new(|| {
            inputs.accent_ink(
                inputs.colors.accent(dark).as_ref(),
                Accent::Primary,
                inputs.colors.container.map(|f| f.ground),
                ink::TEXT,
            )
        });
        let mut button = |b: &Button, x: Option<f32>, right: &mut f32, left: &mut f32| {
            let ink = match (b.enabled, b.tint, b.symbol.is_some(), content) {
                (false, _, true, Some(ink)) => disabled(ink),
                (false, ..) => Role::Disabled.color(dark),
                (true, Some(tint), ..) => tint,
                (true, _, true, Some(ink)) => ink,
                _ if b.destructive => inputs.accent_ink(
                    None,
                    Accent::Error,
                    inputs.colors.container.map(|f| f.ground),
                    if b.symbol.is_some() {
                        ink::SYMBOL
                    } else {
                        ink::TEXT
                    },
                ),
                (_, _, true, _) => Role::OnSurfaceVariant.color(dark),
                _ => *primary,
            };
            let label = (b.badge.is_none() && b.symbol.is_none()).then(|| {
                let room = (inputs.width / 2.0 - 2.0 * TEXT_BUTTON_PADDING).max(1.0);
                line(engine, &b.title, LABEL_LARGE, scale, room, b.direction)
            });
            let w = match &label {
                Some(p) => (p.width + 2.0 * TEXT_BUTTON_PADDING).max(TEXT_BUTTON.0),
                None => ICON_BUTTON,
            };
            // Leading: placed at `x` and the line moves on. Trailing: it
            // ends where the one after it begins.
            let at = match x {
                Some(x) => {
                    *left = x + w;
                    x
                }
                None => {
                    *right -= w;
                    *right
                }
            };
            let (layer, layer_ink) = match (&b.badge, &b.symbol, &label) {
                (Some(badge), _, _) => {
                    let rect = (
                        at + (ICON_BUTTON - badge.size) / 2.0,
                        middle(badge.size),
                        badge.size,
                        badge.size,
                    );
                    items.push(Item::Badge(DrawnBadge::layout(
                        engine, badge, rect, b.enabled, scale,
                    )));
                    (circle(at), title_ink)
                }
                (None, Some(role), _) => {
                    let rect = (at + (ICON_BUTTON - icon) / 2.0, middle(icon), icon, icon);
                    items.push(Item::Symbol(role.clone(), rect, ink, b.symbol_direction));
                    (circle(at), ink)
                }
                (None, None, Some(p)) => {
                    let tall = (p.height + 20.0).max(TEXT_BUTTON.1);
                    let text = (label_x(p, at + w / 2.0, 0.5), middle(p.height));
                    items.push(Item::Text(p.clone(), text, ink, b.node, None));
                    (
                        Shape::new((at, middle(tall), w, tall), [tall / 2.0; 4]),
                        ink,
                    )
                }
                (None, None, None) => (circle(at), ink),
            };
            targets.push(Target {
                rect: (at, middle(ICON_BUTTON), w, ICON_BUTTON),
                what: What::Item(b.node),
                enabled: b.enabled,
                layer: (layer, layer_ink),
            });
        };
        for b in &top.leading {
            let x = left;
            button(b, Some(x), &mut right, &mut left);
        }
        for b in top.trailing.iter().rev() {
            button(b, None, &mut right, &mut left);
        }
        // The trailing buttons were measured last to first: back in the
        // order the author wrote them, after the leading ones.
        let first = targets.len() - top.trailing.len();
        targets[first..].reverse();
        // The title's box is what leads and what trails leave it, its text
        // `edge` inside it. It starts at the inset or after what leads,
        // whichever is further, and moves back from what trails where it
        // would reach it (`TopAppBarLayout`).
        let room = right - left - 2.0 * edge;
        if let Some(heading) = top.heading.filter(|_| !top.title.is_empty() && room > 0.0) {
            let p = line(
                engine,
                &top.title,
                TITLE_LARGE,
                scale,
                room,
                top.title_direction,
            );
            let bounds = text_rect(&p, (0., 0.));
            let sub = has_subtitle.then(|| {
                let sub_p = line(
                    engine,
                    &top.subtitle,
                    LABEL_MEDIUM,
                    scale,
                    room,
                    top.subtitle_direction,
                );
                let sub_bounds = text_rect(&sub_p, (0., 0.));
                (top.subtitle_node.unwrap(), sub_p, sub_bounds)
            });
            let col_w = sub
                .as_ref()
                .map_or(bounds.2, |(_, _, sub_bounds)| bounds.2.max(sub_bounds.2));
            let x = ((inputs.sides.0 + dp(metric::TOP_TITLE_INSET)).max(left) + edge)
                .min(right - edge - col_w)
                .max(left + edge);
            let slot = Some((left + edge, y0, room, h));
            if let Some((sub_node, sub_p, _)) = sub {
                let sub_ink = content.unwrap_or_else(|| Role::OnSurfaceVariant.color(dark));
                let col_y = middle(p.height + sub_p.height);
                items.push(Item::Text(
                    p.clone(),
                    (label_x(&p, x, 0.), col_y),
                    title_ink,
                    heading,
                    slot,
                ));
                items.push(Item::Text(
                    sub_p.clone(),
                    (label_x(&sub_p, x, 0.), col_y + p.height),
                    sub_ink,
                    sub_node,
                    slot,
                ));
            } else {
                let at = (label_x(&p, x, 0.), middle(p.height));
                items.push(Item::Text(p, at, title_ink, heading, slot));
            }
        }
        if top.direction == Direction::Rtl {
            mirror(&mut items, &mut targets, inputs.width);
        }
        Layout {
            size: (inputs.width, y0 + h),
            container,
            items,
            targets,
        }
    }

    /// A destination's badge on its anchor (its icon, else its label), Material's large badge in a
    /// `BadgedBox` (Compose Material 3 1.4, `Badge.kt`, `BadgeTokens`): a
    /// pill at least 16 square in `error`, its text label-small in
    /// `onError` 4 from its sides; its start 12 before the anchor's end and
    /// its bottom 14 into the anchor.
    fn badge(
        engine: &mut TextEngine,
        text: &str,
        anchor: Rect4,
        node: ViewId,
        inputs: &Inputs,
        items: &mut Vec<Item>,
    ) {
        let dark = inputs.dark;
        let p = paragraph(engine, text, LABEL_SMALL, inputs.font_scale, None);
        let h = BADGE.max(p.height);
        let w = (p.width + 2.0 * BADGE_PADDING).max(h);
        let pill = (
            anchor.0 + anchor.2 - BADGE_OFFSET.0,
            anchor.1 - h + BADGE_OFFSET.1,
            w,
            h,
        );
        let fill = told(Material::Error, dark, 0xFFB3_261E, 0xFFF2_B8B5);
        items.push(Item::Fill(Shape::new(pill, [h / 2.0; 4]), fill));
        let ink = told(Material::OnError, dark, 0xFFFF_FFFF, 0xFF60_1410);
        let at = (
            label_x(&p, pill.0 + w / 2.0, 0.5),
            pill.1 + (h - p.height) / 2.0,
        );
        items.push(Item::Text(p, at, ink, node, None));
    }

    /// Material's navigation bar for `bottom`.
    pub fn bottom(engine: &mut TextEngine, bottom: &Bottom, inputs: &Inputs) -> Layout {
        let inputs = if bottom.direction == Direction::Rtl {
            &Inputs {
                sides: (inputs.sides.1, inputs.sides.0),
                ..*inputs
            }
        } else {
            inputs
        };
        let (dark, scale, nav) = (inputs.dark, inputs.font_scale, nav());
        let h = nav_height(scale);
        let content = nav.icon + nav.indicator_padding + nav.label_gap + LABEL_MEDIUM.1 * scale;
        let padding = (h - content) / 2.0;
        // The destinations share what the gaps between them leave (the
        // `Row`'s `Arrangement.spacedBy`, each item's `weight(1f)`).
        let span = (inputs.width - inputs.sides.0 - inputs.sides.1).max(0.0);
        let count = bottom.tabs.len().max(1) as f32;
        let cell = ((span - nav.item_gap * (count - 1.0)) / count).max(0.0);
        let container = inputs
            .colors
            .container
            .map_or_else(|| Role::SurfaceContainer.color(dark), |f| f.paint);
        let accent = inputs.colors.accent(dark);
        let indicator = accent.as_ref().map_or_else(
            || Role::SecondaryContainer.color(dark),
            |s| opaque(s.secondary_container()),
        );
        let inactive = inputs
            .colors
            .ink
            .unwrap_or_else(|| told(Material::OnSurfaceVariant, dark, 0xFF49_454F, 0xFFCA_C4D0));
        let (mut items, mut targets) = (Vec::new(), Vec::new());
        for (index, tab) in bottom.tabs.iter().enumerate() {
            let x = inputs.sides.0 + (cell + nav.item_gap) * index as f32;
            let middle = x + cell / 2.0;
            let (ink, text) = if tab.selected && (tab.enabled || accent.is_some()) {
                let bar_ground = inputs.colors.container.map(|f| f.ground);
                // A translucent ButtonFace exposes the authored bar through the indicator.
                let pill_ground = bar_ground
                    .filter(|_| indicator[3] < 255)
                    .map(|bar| ink::over(indicator, bar));
                let symbol = inputs.accent_ink(
                    accent.as_ref(),
                    Accent::OnSecondaryContainer,
                    pill_ground,
                    ink::SYMBOL,
                );
                let label_ground = if tab.symbol.is_some() {
                    bar_ground
                } else {
                    pill_ground
                };
                let mut label = inputs.accent_ink(
                    accent.as_ref(),
                    Accent::Secondary,
                    label_ground.filter(|_| tab.enabled),
                    ink::TEXT,
                );
                // On an opaque seeded indicator a lone label may need the pill's own on-colour.
                if accent.is_some()
                    && tab.symbol.is_none()
                    && material_colors::contrast::ratio_of_tones(
                        Rgb::new(label[0], label[1], label[2]).as_lstar(),
                        Rgb::new(indicator[0], indicator[1], indicator[2]).as_lstar(),
                    ) < f64::from(ink::TEXT)
                {
                    label = symbol;
                }
                if tab.enabled {
                    (symbol, label)
                } else {
                    (disabled(symbol), disabled(label))
                }
            } else {
                let ink = if tab.enabled {
                    inactive
                } else {
                    disabled(inactive)
                };
                (ink, ink)
            };
            let label = (!tab.label.is_empty())
                .then(|| line(engine, &tab.label, LABEL_MEDIUM, scale, cell, tab.direction));
            let pill = (
                middle - nav.indicator.0 / 2.0,
                padding - nav.indicator_padding,
                nav.indicator.0,
                nav.indicator.1,
            );
            let mut layer = Shape::new(pill, [nav.indicator.1 / 2.0; 4]);
            match (&tab.symbol, &label) {
                (Some(role), _) => {
                    if tab.selected {
                        items.push(Item::Fill(layer, indicator));
                    }
                    let icon = (middle - nav.icon / 2.0, padding, nav.icon, nav.icon);
                    items.push(Item::Symbol(role.clone(), icon, ink, tab.symbol_direction));
                    if let Some(badge) = &tab.badge {
                        Self::badge(engine, badge, icon, tab.node, inputs, &mut items);
                    }
                    if let Some(p) = &label {
                        let y = padding + nav.icon + nav.indicator_padding + nav.label_gap;
                        items.push(Item::Text(
                            p.clone(),
                            (label_x(p, middle, 0.5), y),
                            text,
                            tab.node,
                            None,
                        ));
                    }
                }
                // A destination with no symbol is its label, in the middle.
                (None, Some(p)) => {
                    let at = (label_x(p, middle, 0.5), (h - p.height) / 2.0);
                    let bounds = text_rect(p, at);
                    let around = (
                        bounds.0 - 12.0,
                        at.1 - 8.0,
                        bounds.2 + 24.0,
                        p.height + 16.0,
                    );
                    layer = Shape::new(around, [around.3 / 2.0; 4]);
                    if tab.selected {
                        items.push(Item::Fill(layer, indicator));
                    }
                    items.push(Item::Text(p.clone(), at, text, tab.node, None));
                    // A destination of a label alone: the label is the badge's anchor, as the icon is.
                    if let Some(badge) = &tab.badge {
                        let anchor = text_rect(p, at);
                        Self::badge(engine, badge, anchor, tab.node, inputs, &mut items);
                    }
                }
                (None, None) => {}
            }
            // A touch between two destinations is the nearer one's, as a
            // destination narrower than 48 takes touches past its sides.
            let half = nav.item_gap / 2.0;
            let start = if index == 0 { inputs.sides.0 } else { x - half };
            let end = match index + 1 == bottom.tabs.len() {
                true => inputs.width - inputs.sides.1,
                false => x + cell + half,
            };
            targets.push(Target {
                rect: (start, 0.0, end - start, h),
                what: What::Tab(tab.node),
                enabled: tab.enabled,
                layer: (
                    layer,
                    if tab.selected {
                        Role::OnSurface.color(dark)
                    } else {
                        inputs
                            .colors
                            .ink
                            .unwrap_or_else(|| Role::OnSurface.color(dark))
                    },
                ),
            });
        }
        if bottom.direction == Direction::Rtl {
            mirror(&mut items, &mut targets, inputs.width);
        }
        Layout {
            size: (inputs.width, h + inputs.edge),
            container,
            items,
            targets,
        }
    }
}

#[cfg(test)]
mod ink_tests;

#[cfg(test)]
mod tests;
