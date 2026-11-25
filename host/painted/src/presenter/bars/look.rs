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
use crate::paint::{Backend, Rect4, Shape};
use crate::presenter::palette::{material, Material};
use crate::presenter::top::look::{
    directed_paragraph, label_x, paragraph, symbol, text_rect, Role, Type, LABEL_LARGE, STATE_LAYER,
};
use crate::text::{Paragraph, RunPaint, TextEngine};
use exact_kernel::generated::metric;
use exact_kernel::style::metrics::{self, Column};
use exact_kernel::{Direction, ViewId};
use material_colors::{color::Rgb, dynamic_color::DynamicScheme, scheme::variant::SchemeTonalSpot};
use std::rc::Rc;
use tiny_skia::Transform;

pub(super) mod ink;
use ink::Accent;

const TITLE_LARGE: Type = Type(22.0, 28.0, 400, 0.0);
const LABEL_MEDIUM: Type = Type(12.0, 16.0, 500, 0.5);
const LABEL_SMALL: Type = Type(11.0, 16.0, 500, 0.5);

/// The top app bar's height, under the status bar.
pub fn top_height() -> f32 {
    dp(metric::TOP_HEIGHT)
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
    /// Whether it has the Up arrow: the route is pushed.
    pub up: bool,
    /// The buttons before the heading, and those after it.
    pub leading: Vec<Button>,
    /// See `leading`.
    pub trailing: Vec<Button>,
    /// Whether the route's content has scrolled under it.
    pub scrolled: bool,
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

enum Item {
    Text(Rc<Paragraph>, (f32, f32), [u8; 4], ViewId),
    Fill(Shape, [u8; 4]),
    Symbol(String, Rect4, [u8; 4]),
}

/// Resolve inline positions into physical boxes without reflecting glyphs.
fn mirror(items: &mut [Item], targets: &mut [Target], width: f32) {
    let rect = |r: &mut Rect4| r.0 = width - r.0 - r.2;
    for item in items {
        match item {
            Item::Text(p, at, ..) => {
                let bounds = text_rect(p, *at);
                at.0 = label_x(p, width - bounds.0, 1.);
            }
            Item::Fill(shape, _) => rect(&mut shape.rect),
            Item::Symbol(_, r, _) => rect(r),
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
    direction: Direction,
    items: Vec<Item>,
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
                Item::Text(p, at, _, source) => Some((*source, text_rect(p, *at))),
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
        // The indicator is under the state layer, and both under the symbol.
        for item in &self.items {
            if let Item::Fill(shape, color) = item {
                backend.fill(shape, *color, ts);
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
                Item::Text(paragraph, at, color, source) => {
                    let paint = [RunPaint {
                        color: *color,
                        source: *source,
                    }];
                    backend.text(engine, paragraph, &paint, *at, ts);
                }
                Item::Fill(..) => {}
                // Material's shapes (D14): the Up arrow is its `back`.
                Item::Symbol(role, rect, color) => symbol(
                    backend,
                    role,
                    *rect,
                    *color,
                    exact_kernel::SymbolSet::Material,
                    400,
                    self.direction == Direction::Rtl && role == "back",
                ),
            }
        }
    }

    /// Material's small top app bar for `top`.
    pub fn top(engine: &mut TextEngine, top: &Top, inputs: &Inputs) -> Layout {
        let inputs = if top.direction == Direction::Rtl {
            &Inputs {
                sides: (inputs.sides.1, inputs.sides.0),
                ..*inputs
            }
        } else {
            inputs
        };
        let (dark, scale) = (inputs.dark, inputs.font_scale);
        let (y0, h) = (inputs.edge, top_height());
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
            items.push(Item::Symbol("back".into(), at, ink));
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
            let label = (b.symbol.is_none()).then(|| {
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
            let layer = match (&b.symbol, &label) {
                (Some(role), _) => {
                    let rect = (at + (ICON_BUTTON - icon) / 2.0, middle(icon), icon, icon);
                    items.push(Item::Symbol(role.clone(), rect, ink));
                    circle(at)
                }
                (None, Some(p)) => {
                    let tall = (p.height + 20.0).max(TEXT_BUTTON.1);
                    let text = (label_x(p, at + w / 2.0, 0.5), middle(p.height));
                    items.push(Item::Text(p.clone(), text, ink, b.node));
                    Shape::new((at, middle(tall), w, tall), [tall / 2.0; 4])
                }
                (None, None) => circle(at),
            };
            targets.push(Target {
                rect: (at, middle(ICON_BUTTON), w, ICON_BUTTON),
                what: What::Item(b.node),
                enabled: b.enabled,
                layer: (layer, ink),
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
            let x = (inputs.sides.0 + dp(metric::TOP_TITLE_INSET)).max(left) + edge;
            let bounds = text_rect(&p, (0., 0.));
            let at = (
                label_x(&p, x.min(right - edge - bounds.2), 0.),
                middle(p.height),
            );
            items.push(Item::Text(p, at, title_ink, heading));
        }
        if top.direction == Direction::Rtl {
            mirror(&mut items, &mut targets, inputs.width);
        }
        Layout {
            size: (inputs.width, y0 + h),
            direction: top.direction,
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
        items.push(Item::Text(p, at, ink, node));
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
                    items.push(Item::Symbol(role.clone(), icon, ink));
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
                    items.push(Item::Text(p.clone(), at, text, tab.node));
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
            direction: bottom.direction,
            container,
            items,
            targets,
        }
    }
}

#[cfg(test)]
mod ink_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// Three destinations, the first selected and the last disabled, in a
    /// 412 wide window under the light scheme, no palette told.
    fn bar() -> Layout {
        badged(None)
    }

    /// `bar()`, its second destination holding `badge`.
    fn badged(badge: Option<&str>) -> Layout {
        labelled(badge, true)
    }

    /// `badged`, its destinations' symbols left out where `symbols` is false.
    fn labelled(badge: Option<&str>, symbols: bool) -> Layout {
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let tab = |node, label: &str, selected, enabled| Tab {
            node,
            label: label.into(),
            direction: Direction::Ltr,
            symbol: symbols.then(|| "star".into()),
            selected,
            enabled,
            badge: badge.filter(|_| node == 3).map(Into::into),
        };
        let bottom = Bottom {
            tablist: 1,
            direction: Direction::Ltr,
            tabs: vec![
                tab(2, "Home", true, true),
                tab(3, "Starred", false, true),
                tab(4, "Settings", false, false),
            ],
        };
        let inputs = Inputs {
            device_inks: &Default::default(),
            colors: BarColors::default(),
            dark: false,
            font_scale: 1.0,
            width: 412.0,
            edge: 0.0,
            sides: (0.0, 0.0),
        };
        Layout::bottom(&mut engine, &bottom, &inputs)
    }

    #[test]
    fn truncated_rtl_bar_labels_fit_and_center_in_their_targets() {
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let title = "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة";
        let top = Top {
            route: 1,
            heading: Some(2),
            direction: Direction::Rtl,
            title: title.into(),
            title_direction: Direction::Rtl,
            up: false,
            leading: vec![],
            trailing: vec![Button {
                node: 3,
                title: title.into(),
                direction: Direction::Rtl,
                symbol: None,
                enabled: true,
                destructive: false,
                tint: None,
            }],
            scrolled: false,
        };
        for font_scale in [1., 2.] {
            let inputs = Inputs {
                device_inks: &Default::default(),
                colors: BarColors::default(),
                dark: false,
                font_scale,
                width: 300.,
                edge: 0.,
                sides: (0., 0.),
            };
            for symbol in [None, Some("star".into())] {
                let bottom = Bottom {
                    tablist: 4,
                    direction: Direction::Rtl,
                    tabs: vec![Tab {
                        node: 5,
                        label: title.repeat(2),
                        direction: Direction::Rtl,
                        symbol,
                        selected: true,
                        enabled: true,
                        badge: None,
                    }],
                };
                let layout = Layout::bottom(&mut engine, &bottom, &inputs);
                for item in &layout.items {
                    let Item::Text(p, at, ..) = item else {
                        continue;
                    };
                    assert!(p
                        .layout_runs()
                        .flat_map(|r| r.glyphs)
                        .any(|g| g.start == g.end));
                    let (left, _, width, _) = text_rect(p, *at);
                    assert!(left >= -0.01 && left + width <= inputs.width + 0.01);
                    assert!((left + width / 2. - inputs.width / 2.).abs() < 0.01);
                }
            }
            let layout = Layout::top(&mut engine, &top, &inputs);
            for item in &layout.items {
                let Item::Text(p, at, _, node) = item else {
                    continue;
                };
                assert!(p
                    .layout_runs()
                    .flat_map(|r| r.glyphs)
                    .any(|g| g.start == g.end));
                let (left, _, width, _) = text_rect(p, *at);
                let target = layout.targets[0].rect;
                if *node == 3 {
                    assert!(
                        (left + width / 2. - (target.0 + target.2 / 2.)).abs() < 0.01,
                        "button label must be centered at scale {font_scale}"
                    );
                    assert!(left >= target.0 + TEXT_BUTTON_PADDING - 0.01);
                    assert!(left + width <= target.0 + target.2 - TEXT_BUTTON_PADDING + 0.01);
                } else {
                    assert!(left >= target.0 + target.2 + dp(metric::TOP_EDGE) - 0.01);
                    assert!(left + width <= inputs.width - 2. * dp(metric::TOP_EDGE) + 0.01);
                }
            }
        }
    }

    #[test]
    fn rtl_bars_keep_each_labels_authored_direction() {
        let source = r#"routes nav
  tab home "/"
  tab other "/other"
component App
  state edited = false
  action edit
    edited = !edited
  view
    main navigationKey=`${top(nav).id}` direction="rtl"
      column role="tabpanel" id="panel-home"
        column navigationKey=`${top(nav).id}`
          header
            text "Beginning of a very long English heading ending here" role="heading" direction="ltr"
            button press=edit
              text "Beginning of a very long English action ending here" direction="ltr"
          text "Body"
      column role="tabpanel" id="panel-other"
        text "Other body"
      row role="tablist"
        button role="tab" aria-controls="panel-home" aria-selected=true
          text "Beginning of a very long English destination ending here" direction="ltr"
        button "Other" role="tab" aria-controls="panel-other" aria-selected=false
"#;
        for rtl in [false, true] {
            let source = if rtl {
                ["heading", "action", "destination"]
                    .into_iter()
                    .fold(source.to_string(), |s, kind| {
                        s.replace(
                            &format!("Beginning of a very long English {kind} ending here"),
                            "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة",
                        )
                    })
                    .replace("direction=\"ltr\"", "direction=\"rtl\"")
            } else {
                source.to_string()
            };
            let (mut p, error) = crate::presenter::Presenter::boot_with(
                &crate::test_plan(&source),
                (),
                (300., 600.),
                1.,
                std::path::PathBuf::new(),
                crate::presenter::PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.host.set_bar_room(Some((64., 80., 80.))).unwrap();
            let (top, bottom) = p.read_bars();
            let inputs = Inputs {
                device_inks: &Default::default(),
                colors: BarColors::default(),
                dark: false,
                font_scale: 1.,
                width: 300.,
                edge: 0.,
                sides: (0., 0.),
            };
            let mut engine = TextEngine::from_font_files(
                &[std::path::Path::new(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../scripts/fixtures/fonts/assets"
                ))],
                "DejaVu Sans",
            );
            let top = Layout::top(&mut engine, &top.unwrap(), &inputs);
            let bottom = Layout::bottom(&mut engine, &bottom.unwrap(), &inputs);
            let mut checked = 0;
            for layout in [&top, &bottom] {
                let texts = layout.texts();
                for item in &layout.items {
                    if let Item::Text(p, at, _, node) = item {
                        let rect = texts.iter().find(|(id, _)| id == node).unwrap().1;
                        for glyph in p.layout_runs().flat_map(|r| r.glyphs) {
                            assert!(at.0 + glyph.x >= rect.0 - 0.01);
                            assert!(at.0 + glyph.x + glyph.w <= rect.0 + rect.2 + 0.01);
                        }
                        for run in p.layout_runs() {
                            if !run
                                .text
                                .starts_with(if rtl { "بداية" } else { "Beginning" })
                            {
                                continue;
                            }
                            assert_eq!(run.rtl, rtl, "the label keeps its authored direction");
                            let first = run
                                .glyphs
                                .iter()
                                .find(|g| g.start == 0 && g.end > 0)
                                .expect("the first cluster survives truncation");
                            let ellipsis = run
                                .glyphs
                                .iter()
                                .find(|g| g.start == g.end)
                                .expect("the long label has an ellipsis");
                            for glyph in run.glyphs {
                                if rtl {
                                    assert!(glyph.x <= first.x, "first cluster at the right");
                                    assert!(glyph.x >= ellipsis.x, "ellipsis at the left");
                                } else {
                                    assert!(glyph.x >= first.x, "first cluster at the left");
                                    assert!(glyph.x <= ellipsis.x, "ellipsis at the right");
                                }
                            }
                            checked += 1;
                        }
                    }
                }
            }
            assert_eq!(checked, 3);
            assert!(bottom.targets[0].rect.0 > bottom.targets[1].rect.0);
        }
    }

    #[test]
    fn a_title_has_the_room_what_leads_and_trails_leave_it() {
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let title = "A title nearly as wide as its bar";
        let wide =
            crate::presenter::top::look::paragraph(&mut engine, title, TITLE_LARGE, 1.0, None)
                .width;
        let button = Button {
            node: 3,
            title: "Star".into(),
            direction: Direction::Ltr,
            symbol: Some("star".into()),
            enabled: true,
            destructive: false,
            tint: None,
        };
        let top = Top {
            route: 1,
            heading: Some(2),
            direction: Direction::Ltr,
            title: title.into(),
            title_direction: Direction::Ltr,
            up: false,
            leading: Vec::new(),
            trailing: vec![button],
            scrolled: false,
        };
        // Compose's room: the bar less 4 and a 48 button at its end, 4 at
        // its start, and the title's own 4 either side; 2 to spare. From
        // the inset (16) instead, the title would be cut.
        let width = wide + 4.0 + 48.0 + 4.0 + 8.0 + 2.0;
        let inputs = Inputs {
            device_inks: &Default::default(),
            colors: BarColors::default(),
            dark: false,
            font_scale: 1.0,
            width,
            edge: 0.0,
            sides: (0.0, 0.0),
        };
        let bar = Layout::top(&mut engine, &top, &inputs);
        let (p, at) = (bar.items.iter())
            .find_map(|item| match item {
                Item::Text(p, at, _, 2) => Some((p.clone(), *at)),
                _ => None,
            })
            .expect("the title");
        assert_eq!(p.width, wide, "not cut");
        // Moved back from the button: it ends 4 before the button's target.
        let bounds = text_rect(&p, at);
        assert!((bounds.0 + bounds.2 - (width - 4.0 - 48.0 - 4.0)).abs() < 0.01);
    }

    #[test]
    fn bars_without_authored_colours_keep_every_device_paint_role() {
        use super::super::tests::{barred, APP};
        let (mut p, _) = barred(APP);
        let paints = |layout: &Layout| {
            layout
                .items
                .iter()
                .map(|item| match item {
                    Item::Fill(_, c) | Item::Symbol(_, _, c) | Item::Text(_, _, c, _) => *c,
                })
                .collect::<Vec<_>>()
        };
        for dark in [false, true] {
            p.set_system_scheme(dark);
            p.sync_bars();
            let top = &p.bars.top.as_ref().unwrap().layout;
            let bottom = &p.bars.bottom.as_ref().unwrap().layout;
            assert_eq!(top.container, Role::Surface.color(dark));
            assert_eq!(
                paints(top),
                [
                    Role::OnSurfaceVariant.color(dark),
                    Role::OnSurface.color(dark)
                ]
            );
            let inactive = told(Material::OnSurfaceVariant, dark, 0xff49454f, 0xffcac4d0);
            assert_eq!(bottom.container, Role::SurfaceContainer.color(dark));
            assert_eq!(
                paints(bottom),
                [
                    Role::SecondaryContainer.color(dark),
                    told(Material::OnSecondaryContainer, dark, 0xff1d192b, 0xffe8def8),
                    told(Material::Secondary, dark, 0xff625b71, 0xffccc2dc),
                    inactive,
                    inactive,
                ]
            );
            assert_eq!(
                (
                    p.chrome().status_light_icons,
                    p.chrome().navigation_light_icons
                ),
                (dark, dark)
            );
        }
    }

    #[test]
    fn the_root_accent_seeds_only_the_bars_accent_roles() {
        use super::super::tests::{barred, APP};
        let source = APP
            .replace(
                "main navigationKey",
                r##"main accent-color="#245c46" navigationKey"##,
            )
            .replace(r#"image "symbol:more""#, r#"text "More""#)
            .replace(
                r#"header display"#,
                r##"header accent-color="#bb2222" display"##,
            );
        let (mut p, _) = barred(&source);
        // Google's Material Color Utilities, SchemeTonalSpot(#245c46), standard contrast.
        for (dark, primary, indicator, symbol, label) in [
            (
                false,
                0xff1e6a4fu32,
                0xffcfe9dau32,
                0xff354b41u32,
                0xff4c6358u32,
            ),
            (
                true,
                0xff8cd5b4u32,
                0xff354b41u32,
                0xffcfe9dau32,
                0xffb3ccbeu32,
            ),
        ] {
            p.set_system_scheme(dark);
            p.sync_bars();
            let rgba = |argb: u32| {
                let [a, r, g, b] = argb.to_be_bytes();
                [r, g, b, a]
            };
            let top = &p.bars.top.as_ref().unwrap().layout;
            let bottom = &p.bars.bottom.as_ref().unwrap().layout;
            assert!(matches!(&top.items[0], Item::Text(_, _, c, _) if *c == rgba(primary)));
            assert!(matches!(&bottom.items[0], Item::Fill(_, c) if *c == rgba(indicator)));
            assert!(matches!(&bottom.items[1], Item::Symbol(_, _, c) if *c == rgba(symbol)));
            assert!(matches!(&bottom.items[2], Item::Text(_, _, c, _) if *c == rgba(label)));
            assert_eq!(top.container, Role::Surface.color(dark));
            assert_eq!(bottom.container, Role::SurfaceContainer.color(dark));
        }
    }

    #[test]
    fn each_bar_uses_its_own_scheme_with_the_roots_single_seed() {
        use super::super::tests::{barred, APP};
        let source = APP
            .replace(
                "main navigationKey",
                r##"main accent-color="light-dark(#245c46, #bb2222)" navigationKey"##,
            )
            .replace(r#"image "symbol:more""#, r#"text "More""#)
            .replace("header display", r##"header color-scheme="dark" display"##)
            .replace(
                r#"testId="tabs""#,
                r##"testId="tabs" background-color="#4c6358""##,
            );
        let (mut p, _) = barred(&source);
        p.set_system_scheme(false);
        p.sync_bars();
        let top = &p.bars.top.as_ref().unwrap().layout;
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert!(matches!(
            &top.items[0],
            Item::Text(_, _, [0x8c, 0xd5, 0xb4, 255], _)
        ));
        assert!(matches!(
            &bottom.items[1],
            Item::Symbol(_, _, [0x35, 0x4b, 0x41, 255])
        ));
        // Both secondary roles miss on #4c6358. Material's foreground tone is 86.28.
        assert!(matches!(
            &bottom.items[2],
            Item::Text(_, _, [0xc5, 0xde, 0xcf, 255], _)
        ));
    }

    #[test]
    fn a_seeded_bar_tracks_material_contrast_and_respects_action_tints() {
        use super::super::tests::{barred, APP};
        let source = APP
            .replace(
                "main navigationKey",
                r##"main accent-color="#245c46" navigationKey"##,
            )
            .replace(r#"image "symbol:more""#, r#"text "More""#)
            .replace(
                "button popovertarget",
                r##"button color="#bb2222" popovertarget"##,
            );
        let (mut p, _) = barred(&source);
        for (level, indicator) in [
            (0.5, [0x5b, 0x72, 0x66, 255]),
            (1., [0x38, 0x4e, 0x43, 255]),
        ] {
            p.colors.material_contrast = level;
            p.sync_bars();
            let top = &p.bars.top.as_ref().unwrap().layout;
            let bottom = &p.bars.bottom.as_ref().unwrap().layout;
            assert!(matches!(
                &top.items[0],
                Item::Text(_, _, [0xbb, 0x22, 0x22, 255], _)
            ));
            assert!(matches!(&bottom.items[0], Item::Fill(_, c) if *c == indicator));
            assert!(matches!(&bottom.items[1], Item::Symbol(_, _, c) if *c == [255; 4]));
        }
        drop(p);
        let lone = source.replace(r#"          image "symbol:home""#, "");
        let (mut p, _) = barred(&lone);
        p.colors.material_contrast = 1.;
        p.sync_bars();
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert!(matches!(
            &bottom.items[0],
            Item::Fill(_, [0x38, 0x4e, 0x43, 255])
        ));
        assert!(matches!(&bottom.items[1], Item::Text(_, _, c, _) if *c == [255; 4]));
        drop(p);
        for accent in [r##"accent-color="#245c46""##, ""] {
            let source = source
                .replace(r##"accent-color="#245c46""##, accent)
                .replace(
                    r##"button color="#bb2222""##,
                    r##"button disabled=true color="#bb2222""##,
                );
            let (mut p, _) = barred(&source);
            p.sync_bars();
            let top = &p.bars.top.as_ref().unwrap().layout;
            assert!(
                matches!(&top.items[0], Item::Text(_, _, c, _) if *c == Role::Disabled.color(false))
            );
        }
    }

    #[test]
    fn a_disabled_selected_symbol_keeps_its_on_colour_over_the_seeded_indicator() {
        use super::super::tests::{barred, APP};
        let source = APP
            .replace(
                "main navigationKey",
                r##"main accent-color="transparent" navigationKey"##,
            )
            .replace(r#"testId="tabs""#, r##"testId="tabs" color="#ffd9e2""##)
            .replace(r#"testId="tab-home""#, r#"testId="tab-home" disabled=true"#);
        let (mut p, _) = barred(&source);
        p.sync_bars();
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert!(matches!(
            &bottom.items[0],
            Item::Fill(_, [0xff, 0xd9, 0xe2, 255])
        ));
        assert!(matches!(
            &bottom.items[1],
            Item::Symbol(_, _, [0x5a, 0x3f, 0x47, 97])
        ));
        assert!(matches!(
            &bottom.items[2],
            Item::Text(_, _, [0x74, 0x56, 0x5f, 97], _)
        ));
    }

    #[test]
    fn the_navigation_bar_has_materials_indicator_and_colours() {
        let bar = bar();
        let mut texts = Vec::new();
        let mut symbols = Vec::new();
        let mut fills = Vec::new();
        for item in &bar.items {
            match item {
                Item::Text(_, _, color, node) => texts.push((*node, *color)),
                Item::Symbol(_, rect, color) => symbols.push((*rect, *color)),
                Item::Fill(shape, _) => fills.push(shape.rect),
            }
        }
        // Where no palette was told, Material's baseline: the selected
        // label `secondary`, its symbol `onSecondaryContainer`; the others
        // `onSurfaceVariant`, at 38 % when disabled.
        let variant = [0x49, 0x45, 0x4F, 0xFF];
        assert_eq!(
            texts,
            [
                (2, [0x62, 0x5B, 0x71, 0xFF]),
                (3, variant),
                (4, [0x49, 0x45, 0x4F, 97])
            ]
        );
        assert_eq!(symbols[0].1, [0x1D, 0x19, 0x2B, 0xFF]);
        assert_eq!(symbols[1].1, variant);
        // One indicator, 56 by 32, behind the first symbol: the first cell
        // is (412 − 16) / 3 = 132 wide.
        assert_eq!(fills.len(), 1);
        let (x, _, w, h) = fills[0];
        assert_eq!((w, h), (56.0, 32.0));
        assert_eq!(x + w / 2.0, 66.0);
        assert_eq!(symbols[2].0 .0 + 12.0, 280.0 + 66.0);
    }

    #[test]
    fn a_destinations_badge_is_materials_large_badge_on_its_icon() {
        let bar = badged(Some("12"));
        let icon = bar
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Symbol(_, rect, _) => Some(*rect),
                _ => None,
            })
            .nth(1)
            .expect("the second destination's icon");
        let pill = bar
            .items
            .iter()
            .find_map(|item| match item {
                Item::Fill(shape, color) if *color == [0xB3, 0x26, 0x1E, 0xFF] => Some(shape.rect),
                _ => None,
            })
            .expect("one badge, in `error`");
        // 12 before the icon's end, 14 into it, 16 tall, wider for two digits.
        assert_eq!(
            (pill.0, pill.1 + pill.3, pill.3),
            (icon.0 + icon.2 - 12.0, icon.1 + 14.0, 16.0)
        );
        assert!(pill.2 > 16.0, "{pill:?}");
        assert!(
            bar.items
                .iter()
                .any(|item| matches!(item, Item::Text(_, _, [255, 255, 255, 255], 3))),
            "its count in `onError`"
        );
    }
    #[test]
    fn a_destination_of_a_label_alone_keeps_its_badge_on_the_label() {
        let bar = labelled(Some("3"), false);
        let label = bar
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(p, at, color, 3) if *color != [255, 255, 255, 255] => {
                    Some(text_rect(p, *at))
                }
                _ => None,
            })
            .expect("the label");
        let pill = bar
            .items
            .iter()
            .find_map(|item| match item {
                Item::Fill(shape, color) if *color == [0xB3, 0x26, 0x1E, 0xFF] => Some(shape.rect),
                _ => None,
            })
            .expect("its badge");
        assert_eq!(
            (pill.0, pill.1 + pill.3),
            (label.0 + label.2 - 12.0, label.1 + 14.0)
        );
    }
}
