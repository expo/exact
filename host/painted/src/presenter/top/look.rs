//! Platform dialogs and menus, measured and painted. Windows uses WinUI
//! metrics; Android uses Material 3 (LLP
//! 1115.001 D10, D1.3): the two surfaces the top layer shows on Android.
//! Painted, not built of framework widgets: the window is the platform's
//! (`Dialog`, `PopupWindow`), and what is in it is drawn here from the
//! reference's own metrics, the Compose Material 3 library's (1.4:
//! `AlertDialog`, `DropdownMenu`, `DropdownMenuItem`), in the device's
//! colours.
//!
//! Lengths are dp, which are this host's points. Type is sp, a point times
//! the person's font scale, taken linearly: Android's own scaling is
//! non-linear above 100%, and comes with the reported type ramp (D4, S2).
//! The face is the page's default family.
use super::shape::{Entry, Plan, Row};
use crate::paint::{rgba, Backend, Ink, Rect4, Shape, SvgPaint};
use crate::text::{Paragraph, RunPaint, TextEngine};
use exact_kernel::style::roles;
use exact_kernel::{ColorValue, Direction, StyleProps, SymbolSet, TextAlign, ViewId, WhiteSpace};
use std::rc::Rc;
use tiny_skia::Transform;

mod menu;
mod windows;

/// A Material colour role, read through the role of Exact's that LLP
/// 1115.001 D4's table gives it (`presenter/palette.rs`): the device's
/// colour where its shell reports a palette, the table's pair elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::presenter) enum Role {
    Surface,
    SurfaceContainerLow,
    SurfaceContainer,
    SurfaceContainerHigh,
    OnSurface,
    OnSurfaceVariant,
    /// `onSurface` at 38%: Material's disabled content.
    Disabled,
    Primary,
    SecondaryContainer,
    Error,
    OutlineVariant,
}

impl Role {
    pub(in crate::presenter) fn color(self, dark: bool) -> [u8; 4] {
        let name = match self {
            Role::Surface => "Canvas",
            Role::SurfaceContainerLow => "-exact-secondary-background",
            Role::SurfaceContainer => "-exact-secondary-grouped-background",
            Role::SurfaceContainerHigh => "-exact-tertiary-grouped-background",
            Role::OnSurface => "-exact-label",
            Role::OnSurfaceVariant => "-exact-secondary-label",
            Role::Disabled => "-exact-tertiary-label",
            Role::Primary => "AccentColor",
            Role::SecondaryContainer => "ButtonFace",
            Role::Error => "-exact-system-red",
            Role::OutlineVariant => "-exact-separator",
        };
        roles::role(name).map_or([0, 0, 0, 255], |id| {
            rgba(ColorValue::Role(id).resolve(dark))
        })
    }
}

/// A Material type style: size and line height in sp, weight, tracking in sp.
#[derive(Clone, Copy)]
pub(in crate::presenter) struct Type(pub f32, pub f32, pub u16, pub f32);
impl Type {
    fn emboldened(self, on: bool) -> Self {
        Self(self.0, self.1, weight(self.2, on), self.3)
    }
}

fn weight(regular: u16, bold_text: bool) -> u16 {
    if bold_text {
        exact_kernel::bolder(regular)
    } else {
        regular
    }
}

const HEADLINE_SMALL: Type = Type(24.0, 32.0, 400, 0.0);
const BODY_MEDIUM: Type = Type(14.0, 20.0, 400, 0.25);
pub(in crate::presenter) const LABEL_LARGE: Type = Type(14.0, 20.0, 500, 0.1);

/// The dialog's padding on every side, and the space under its headline
/// and under its supporting text.
const DIALOG_PADDING: f32 = 24.0;
const DIALOG_TITLE_GAP: f32 = 16.0;
const DIALOG_TEXT_GAP: f32 = 24.0;
/// The dialog is 280 to 560 wide, and keeps this far from the window's sides.
const DIALOG_WIDTH: (f32, f32) = (280.0, 560.0);
const DIALOG_MARGIN: f32 = 24.0;
/// A text button: at least 58 by 40, its label 12 from its sides; 8 between
/// two on a line, 12 between two lines.
const BUTTON_MIN: (f32, f32) = (58.0, 40.0);
const BUTTON_PADDING: f32 = 12.0;
const BUTTON_GAP: (f32, f32) = (8.0, 12.0);
/// The menu is 112 to 280 wide, 8 above its first row and under its last;
/// a row is 48 tall, its content 12 from its sides, a symbol 24 with 12
/// after it; a separator is a 1 dp line with 8 above and under it.
const MENU_WIDTH: (f32, f32) = (112.0, 280.0);
const MENU_PADDING: f32 = 8.0;
const MENU_ROW: f32 = 48.0;
const MENU_INSET: f32 = 12.0;
const MENU_SYMBOL: f32 = 24.0;
const MENU_RULE: (f32, f32) = (1.0, 8.0);
/// A section's title is a 32 dp line of label type, inset as an item's
/// label, in the variant's colour: it names the items under it and takes no
/// press.
const MENU_HEADER: f32 = 32.0;
/// Material's state layer under a keyboard's focus, and under a press that
/// shows no ripple.
pub(in crate::presenter) const STATE_LAYER: f32 = 0.10;

/// What a surface's look depends on besides what it says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    /// The scheme is dark.
    pub dark: bool,
    /// The presented element's computed direction.
    pub direction: Direction,
    /// The system contrast theme, independent of light or dark appearance.
    pub contrast: Option<crate::presenter::ContrastPalette>,
    /// The person's font scale.
    pub font_scale: f32,
    /// Bold Text applies to the surface's default system font.
    pub bold_text: bool,
    /// The window it is shown over, points.
    pub window: (f32, f32),
    /// The height a menu may take: the window's, less what it keeps from
    /// the window's top and bottom. A taller one scrolls.
    pub room: f32,
}

/// One thing a surface draws, in its own points.
enum Item {
    Text(Rc<Paragraph>, (f32, f32), [u8; 4], ViewId),
    Rule(Rect4, [u8; 4]),
    Symbol(String, Rect4, [u8; 4]),
}

#[derive(Clone, Copy, Default)]
pub(in crate::presenter) struct Feedback {
    pub pressed: Option<usize>,
    pub focused: Option<usize>,
    pub hovered: Option<usize>,
    /// A finger's ripple on a Material surface's target (LLP 1115.001 D9):
    /// the target, the ripple, the instant and whether motion is removed.
    /// It takes the place of the pressed target's state layer.
    pub ripple: Option<(usize, crate::paint::ripple::Ripple, f64, bool)>,
}

/// A row as it can be touched: its box, the authored node it stands for,
/// and the shape and colour its press and its focus show in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    /// Its box in the surface, points.
    pub rect: Rect4,
    /// The authored `button`.
    pub node: ViewId,
    /// Whether it takes a press.
    pub enabled: bool,
    layer: (Shape, [u8; 4]),
}

impl Target {
    /// A point of the surface as an offset into the shape its press shows in.
    pub(in crate::presenter) fn held_at(&self, x: f32, y: f32) -> (f32, f32) {
        let (left, top, ..) = self.layer.0.rect;
        (x - left, y - top)
    }
}

/// A surface measured: its size, its container and what it draws.
pub struct Layout {
    /// Width and height, points.
    pub size: (f32, f32),
    /// The container's corner radius, and its elevation (a shadow the
    /// window casts): 28 and none for a dialog, 4 and 3 for a menu.
    pub radius: f32,
    /// See `radius`.
    pub elevation: f32,
    container: [u8; 4],
    border: Option<[u8; 4]>,
    contrast: Option<crate::presenter::ContrastPalette>,
    items: Vec<Item>,
    /// The rows, in the plan's order.
    pub targets: Vec<Target>,
    windows: Option<windows::Content>,
    menu: Option<menu::Scroll>,
    /// Whose shapes its symbols take: WinUI's on the desktop, else Material's.
    symbols: SymbolSet,
    symbol_weight: u16,
}

pub(in crate::presenter) fn paragraph(
    engine: &mut TextEngine,
    text: &str,
    Type(size, line, weight, tracking): Type,
    scale: f32,
    width: Option<f32>,
) -> Rc<Paragraph> {
    directed_paragraph(
        engine,
        text,
        Type(size, line, weight, tracking),
        scale,
        width,
        Direction::Ltr,
    )
}

pub(in crate::presenter) fn directed_paragraph(
    engine: &mut TextEngine,
    text: &str,
    Type(size, line, weight, tracking): Type,
    scale: f32,
    width: Option<f32>,
    direction: Direction,
) -> Rc<Paragraph> {
    let style = StyleProps::default().with_rows(|rows| {
        rows.font_size = size * scale;
        rows.font_weight = weight;
        rows.line_height = exact_kernel::LineHeight::Length(line * scale);
        rows.letter_spacing = tracking * scale;
        rows.white_space = match width {
            Some(_) => WhiteSpace::Normal,
            None => WhiteSpace::Nowrap,
        };
        rows.direction = direction;
        rows.text_align = TextAlign::Start;
    });
    engine.paragraph(&crate::paint::text_spec(&style, text), width)
}

/// One line, cut with an ellipsis where it is wider than `max`.
fn label(engine: &mut TextEngine, row: &Row, inputs: &Inputs, max: f32) -> Rc<Paragraph> {
    let line = directed_paragraph(
        engine,
        &row.title,
        LABEL_LARGE.emboldened(inputs.bold_text),
        inputs.font_scale,
        None,
        row.direction,
    );
    line.ellipsized(max).unwrap_or(line)
}

/// Content width omits the offset from alignment or RTL truncation.
pub(in crate::presenter) fn text_rect(p: &Paragraph, at: (f32, f32)) -> Rect4 {
    let (left, right) = p
        .layout_runs()
        .flat_map(|run| run.glyphs)
        .map(|g| (g.x, g.x + g.w))
        .reduce(|(left, right), (x, end)| (left.min(x), right.max(end)))
        .unwrap_or((0., p.width));
    (at.0 + left, at.1, right - left, p.height)
}

/// Place a single line's glyph bounds at a physical anchor: 0 is its left
/// edge, 0.5 its center, and 1 its right edge. RTL truncation can leave a
/// nonzero left offset even when the paragraph reports its content width.
pub(in crate::presenter) fn label_x(p: &Paragraph, anchor: f32, alignment: f32) -> f32 {
    let (left, _, width, _) = text_rect(p, (0., 0.));
    anchor - width * alignment - left
}

impl Layout {
    /// Each text it draws, by the node it stands for, with its box in the
    /// surface, points: what an accessibility service reads there.
    pub fn texts(&self) -> Vec<(ViewId, Rect4)> {
        self.items
            .iter()
            .filter_map(|item| match item {
                Item::Text(p, at, _, source) => Some((*source, text_rect(p, *at))),
                _ => None,
            })
            .chain(self.windows.iter().flat_map(|content| content.texts()))
            .collect()
    }

    /// Measure `plan` for `inputs`.
    pub fn of(engine: &mut TextEngine, plan: &Plan, pop: ViewId, inputs: &Inputs) -> Layout {
        match plan {
            Plan::Alert {
                title,
                title_direction,
                message,
                message_directions,
                rows,
                ..
            } => alert(
                engine,
                (title.as_deref(), message.as_deref()),
                (*title_direction, message_directions),
                rows,
                pop,
                inputs,
            ),
            Plan::Menu(entries) => menu(engine, entries, inputs, false),
        }
    }

    /// Windows' ContentDialog and MenuFlyout presentation.
    pub fn desktop(engine: &mut TextEngine, plan: &Plan, pop: ViewId, inputs: &Inputs) -> Layout {
        match plan {
            Plan::Menu(entries) => menu(engine, entries, inputs, true),
            Plan::Alert {
                title,
                message,
                rows,
                ..
            } => windows::alert(
                engine,
                (title.as_deref(), message.as_deref()),
                rows,
                pop,
                inputs,
            ),
        }
    }

    /// The row at a point of the surface.
    pub fn target_at(&self, x: f32, y: f32) -> Option<usize> {
        if self.menu_clip().is_some_and(|r| !menu::contains(r, x, y)) {
            return None;
        }
        self.targets.iter().position(|t| {
            x >= t.rect.0 && y >= t.rect.1 && x < t.rect.0 + t.rect.2 && y < t.rect.1 + t.rect.3
        })
    }

    /// Paint the surface into `backend`, its top left at the origin:
    /// the container, each control's feedback, then its text.
    pub fn paint(&self, backend: &mut dyn Backend, engine: &mut TextEngine, feedback: Feedback) {
        let ts = Transform::identity();
        let container = Shape::new((0.0, 0.0, self.size.0, self.size.1), [self.radius; 4]);
        backend.fill(&container, self.container, ts);
        if let Some(content) = &self.windows {
            content.paint(backend, engine, &container);
        }
        if let Some(border) = self.border {
            for part in crate::paint::border::border_fills(
                &container,
                [if self.contrast.is_some() { 2. } else { 1. }; 4],
                [border; 4],
            ) {
                backend.fill_border(&part, ts);
            }
        }
        if let Some(content) = &self.windows {
            content.paint_buttons(backend, &self.targets, feedback);
        }
        if let Some(clip) = self.menu_clip() {
            backend.push_clip(&container, ts);
            backend.push_clip(&Shape::rect(clip), ts);
        }
        let ripple = feedback.ripple.filter(|_| self.windows.is_none());
        let lit = feedback
            .focused
            .into_iter()
            .chain(feedback.pressed.filter(|i| {
                self.windows.is_none() && Some(*i) != feedback.focused && ripple.is_none()
            }));
        for target in lit.filter_map(|i| self.targets.get(i)) {
            let (shape, mut tint) = target.layer;
            if self.windows.is_some() {
                let (x, y, w, h) = target.rect;
                let ring = Shape::new((x - 3., y - 3., w + 6., h + 6.), [7.; 4]);
                for part in crate::paint::border::border_fills(&ring, [2.; 4], [tint; 4]) {
                    backend.fill_border(&part, ts);
                }
                continue;
            }
            if let Some(palette) = self.contrast {
                tint = palette.highlight;
            } else {
                tint[3] = (f32::from(tint[3]) * STATE_LAYER).round() as u8;
            }
            backend.fill(&shape, tint, ts);
        }
        if let Some((target, ripple, now, still)) =
            ripple.and_then(|(i, r, now, still)| Some((self.targets.get(i)?, r, now, still)))
        {
            let (shape, tint) = target.layer;
            let wave = ripple.wave(shape.rect, now, still);
            crate::paint::ripple::fill(backend, &shape, tint, wave, ts);
        }
        for item in &self.items {
            match item {
                Item::Text(paragraph, at, color, source) => {
                    let color = self
                        .windows
                        .as_ref()
                        .and_then(|content| {
                            feedback
                                .pressed
                                .or(feedback.hovered.filter(|_| self.contrast.is_some()))
                                .and_then(|i| self.targets.get(i))
                                .filter(|target| target.node == *source && target.enabled)
                                .map(|_| content.pressed_ink())
                        })
                        .or_else(|| {
                            self.contrast
                                .filter(|_| {
                                    self.windows.is_none()
                                        && self.targets.iter().enumerate().any(|(i, t)| {
                                            t.node == *source
                                                && t.enabled
                                                && (feedback.focused == Some(i)
                                                    || feedback.pressed == Some(i))
                                        })
                                })
                                .map(|p| p.highlighted_text)
                        })
                        .unwrap_or(*color);
                    let paint = [RunPaint {
                        color,
                        source: *source,
                    }];
                    backend.text(engine, paragraph, &paint, *at, ts);
                }
                Item::Rule(rect, color) => backend.fill(&Shape::rect(*rect), *color, ts),
                Item::Symbol(role, rect, color) => {
                    let color = self
                        .contrast
                        .filter(|_| {
                            self.windows.is_none()
                                && self.targets.iter().enumerate().any(|(i, t)| {
                                    t.enabled
                                        && rect.1 >= t.rect.1
                                        && rect.1 < t.rect.1 + t.rect.3
                                        && (feedback.focused == Some(i)
                                            || feedback.pressed == Some(i))
                                })
                        })
                        .map_or(*color, |p| p.highlighted_text);
                    symbol(
                        backend,
                        role,
                        *rect,
                        color,
                        self.symbols,
                        self.symbol_weight,
                        false,
                    );
                }
            }
        }
        if self.menu.is_some() {
            backend.pop_clip();
            backend.pop_clip();
            self.paint_menu_scrollbar(backend);
        }
    }
}

/// A symbol role's path in `rect`, in `set`'s shape, as the page draws one
/// at `weight` (`paint/svg.rs`): stroked, or filled.
pub(in crate::presenter) fn symbol(
    backend: &mut dyn Backend,
    role: &str,
    rect: Rect4,
    color: [u8; 4],
    set: SymbolSet,
    weight: u16,
    mirrored: bool,
) {
    // A role, an icon token's form, or `sf/` a role's Apple name (LLP 1121 D10).
    let Some(path) = exact_kernel::symbol_path_in(role, set) else {
        return;
    };
    // A translucent colour once over all an icon's paths.
    let group = path.d.contains(';') && color[3] < 255;
    if group {
        backend.push_opacity(f32::from(color[3]) / 255.0);
    }
    let paint = if group {
        [color[0], color[1], color[2], 255]
    } else {
        color
    };
    for d in path.d.split(';') {
        glyph_in(backend, d, path, rect, paint, weight, mirrored);
    }
    if group {
        backend.pop_opacity();
    }
}

/// A path in `rect` under its viewBox, stroked in a role's 24 units, or filled.
fn glyph_in(
    backend: &mut dyn Backend,
    d: &str,
    symbol: exact_kernel::SymbolPath<'_>,
    rect: Rect4,
    color: [u8; 4],
    weight: u16,
    mirrored: bool,
) {
    let [_, _, bw, _] = crate::paint::view_box(symbol.view_box);
    let path = exact_kernel::svg::parse_d(d);
    let weight = f32::from(weight).clamp(100., 900.);
    let paint = SvgPaint {
        path: &path,
        fill: symbol.filled.then_some(Ink::Solid(color)),
        even_odd: symbol.even_odd,
        stroke: (!symbol.filled).then_some(Ink::Solid(color)),
        order: [0, 1, 2],
        width: (1.1 + (weight - 100.0) / 400.0) * bw / 24.0,
        cap: 1,
        join: 1,
        miter: 4.0,
        dash: Vec::new(),
        phase: 0.0,
    };
    let ts = crate::paint::fitted(symbol.view_box, rect);
    let ts = if mirrored {
        Transform::from_translate(2.0 * rect.0 + rect.2, 0.0)
            .pre_scale(-1.0, 1.0)
            .pre_concat(ts)
    } else {
        ts
    };
    backend.svg_path(&paint, ts);
}

/// Material's basic dialog: the headline, the supporting text, and the
/// buttons as text buttons flowing from the trailing edge, a second line
/// where they do not fit one.
fn alert(
    engine: &mut TextEngine,
    (title, message): (Option<&str>, Option<&str>),
    (title_direction, message_directions): (Direction, &[Direction]),
    rows: &[Row],
    pop: ViewId,
    inputs: &Inputs,
) -> Layout {
    let (dark, scale) = (inputs.dark, inputs.font_scale);
    let room = inputs.window.0 - 2.0 * DIALOG_MARGIN;
    let width = room.clamp(DIALOG_WIDTH.0.min(inputs.window.0), DIALOG_WIDTH.1);
    let inner = (width - 2.0 * DIALOG_PADDING).max(1.0);
    let mut items = Vec::new();
    let mut y = DIALOG_PADDING;
    let title = title.into_iter().map(|text| {
        (
            text.to_string(),
            title_direction,
            HEADLINE_SMALL,
            Role::OnSurface,
            DIALOG_TITLE_GAP,
        )
    });
    let mut pieces = message.into_iter().flat_map(|text| text.split('\n'));
    let mut remaining = message_directions.len();
    let message = message_directions
        .chunk_by(|a, b| a == b)
        .map(|directions| {
            let text = pieces
                .by_ref()
                .take(directions.len())
                .collect::<Vec<_>>()
                .join(" ");
            remaining -= directions.len();
            let gap = if remaining == 0 { DIALOG_TEXT_GAP } else { 0. };
            (
                text,
                directions[0],
                BODY_MEDIUM,
                Role::OnSurfaceVariant,
                gap,
            )
        });
    for (text, direction, style, role, gap) in title.chain(message) {
        let p = directed_paragraph(
            engine,
            &text,
            style.emboldened(inputs.bold_text),
            scale,
            Some(inner),
            direction,
        );
        let height = p.height;
        items.push(Item::Text(p, (DIALOG_PADDING, y), role.color(dark), pop));
        y += height + gap;
    }
    // Each button's label and its box's size, then lines of them.
    let buttons: Vec<(Rc<Paragraph>, f32, f32)> = (rows.iter())
        .map(|row| {
            let p = label(engine, row, inputs, inner - 2.0 * BUTTON_PADDING);
            let w = (p.width + 2.0 * BUTTON_PADDING).max(BUTTON_MIN.0);
            let h = (p.height + 20.0).max(BUTTON_MIN.1);
            (p, w.min(inner), h)
        })
        .collect();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (i, (_, w, _)) in buttons.iter().enumerate() {
        let fits = used + w + if used > 0.0 { BUTTON_GAP.0 } else { 0.0 } <= inner + 0.01;
        if !fits && used > 0.0 {
            lines.push(Vec::new());
            used = 0.0;
        }
        used += w + if used > 0.0 { BUTTON_GAP.0 } else { 0.0 };
        lines.last_mut().expect("a line").push(i);
    }
    let mut targets = vec![None; rows.len()];
    for (n, line) in lines.iter().enumerate() {
        let total: f32 = line.iter().map(|i| buttons[*i].1).sum::<f32>()
            + BUTTON_GAP.0 * line.len().saturating_sub(1) as f32;
        let tall = line.iter().map(|i| buttons[*i].2).fold(0.0, f32::max);
        let mut x = DIALOG_PADDING + inner - total;
        y += if n > 0 { BUTTON_GAP.1 } else { 0.0 };
        for &i in line {
            let (p, w, h) = &buttons[i];
            let row = &rows[i];
            let left = if inputs.direction == Direction::Rtl {
                width - x - w
            } else {
                x
            };
            let rect = (left, y + (tall - h) / 2.0, *w, *h);
            let ink = match (row.enabled, row.destructive) {
                (false, _) => Role::Disabled,
                (true, true) => Role::Error,
                (true, false) => Role::Primary,
            }
            .color(dark);
            let at = (
                label_x(p, rect.0 + w / 2.0, 0.5),
                rect.1 + (h - p.height) / 2.0,
            );
            items.push(Item::Text(p.clone(), at, ink, row.node));
            targets[i] = Some(Target {
                rect,
                node: row.node,
                enabled: row.enabled,
                layer: (Shape::new(rect, [h / 2.0; 4]), ink),
            });
            x += w + BUTTON_GAP.0;
        }
        y += tall;
    }
    Layout {
        size: (width, y + DIALOG_PADDING),
        radius: 28.0,
        elevation: 0.0,
        container: Role::SurfaceContainerHigh.color(dark),
        border: None,
        contrast: None,
        items,
        targets: targets.into_iter().flatten().collect(),
        windows: None,
        menu: None,
        symbols: SymbolSet::Material,
        symbol_weight: 400,
    }
}

/// Material's menu: 48 dp rows, each a label with a leading symbol where
/// its button has one (a check where it is `aria-checked`), a destructive
/// row in the error role, a line where the author wrote a separator, and a
/// section's title over its rows.
fn menu(engine: &mut TextEngine, entries: &[Entry], inputs: &Inputs, desktop: bool) -> Layout {
    let (dark, scale) = (inputs.dark, inputs.font_scale);
    // WinUI's MenuFlyout_themeresources.xaml: narrow input padding, 16 px
    // icons, 28 px placeholder, and a 4,2,4,2 item margin.
    // https://github.com/microsoft/microsoft-ui-xaml/blob/winui3/main/controls/dev/CommonStyles/MenuFlyout_themeresources.xaml
    let inset = if desktop { 16. } else { MENU_INSET };
    let padding = if desktop { 2. } else { MENU_PADDING };
    let symbol = if desktop { 16. } else { MENU_SYMBOL };
    let margin = if desktop { 2. } else { 0. };
    let rule = if desktop { (1., 3.) } else { MENU_RULE };
    let range = if desktop { (120., 328.) } else { MENU_WIDTH };
    let widest = range.1.min(inputs.window.0);
    let lead = |row: &Row| match (&row.symbol, row.checked) {
        (Some(symbol), _) => Some(symbol.clone()),
        (None, true) => Some("checkmark".to_string()),
        (None, false) => None,
    };
    let rows = || {
        entries.iter().filter_map(|e| match e {
            Entry::Item(row) => Some(row),
            _ => None,
        })
    };
    let has_symbols = rows().any(|r| lead(r).is_some());
    let indent = |row: &Row| {
        if desktop {
            if has_symbols {
                28.
            } else {
                0.
            }
        } else {
            match lead(row) {
                Some(_) => MENU_SYMBOL + MENU_INSET,
                None => 0.0,
            }
        }
    };
    // WinUI's MenuFlyout has no section title: one in its caption type.
    let header = if desktop {
        Type(12., 16., 600, 0.)
    } else {
        LABEL_LARGE
    };
    let labels: Vec<Option<Rc<Paragraph>>> = (entries.iter())
        .map(|entry| {
            let room = |indent: f32| (widest - 2.0 * inset - indent).max(1.);
            Some(match entry {
                Entry::Item(row) if desktop => {
                    let p = paragraph(engine, &row.title, Type(14., 20., 400, 0.), scale, None);
                    p.ellipsized(room(indent(row))).unwrap_or(p)
                }
                Entry::Item(row) => label(engine, row, inputs, room(indent(row))),
                Entry::Header(title, _, direction) => {
                    let p = directed_paragraph(
                        engine,
                        title,
                        header.emboldened(inputs.bold_text && !desktop),
                        scale,
                        None,
                        if desktop { Direction::Ltr } else { *direction },
                    );
                    p.ellipsized(room(0.)).unwrap_or(p)
                }
                Entry::Separator => return None,
            })
        })
        .collect();
    let content = (entries.iter().zip(&labels))
        .filter_map(|(entry, p)| {
            let indent = match entry {
                Entry::Item(row) => indent(row),
                _ => 0.,
            };
            Some(indent + p.as_ref()?.width)
        })
        .fold(0.0, f32::max);
    let width = (content + 2.0 * inset).clamp(range.0.min(widest), widest);
    let (mut items, mut targets) = (Vec::new(), Vec::new());
    let mut y = padding;
    for (entry, p) in entries.iter().zip(labels) {
        let (row, p) = match (entry, p) {
            (Entry::Item(row), Some(p)) => (row, p),
            (Entry::Header(_, node, _), Some(p)) => {
                let tall = (p.height + 12.).max(if desktop { 28. } else { MENU_HEADER });
                let ink = inputs.contrast.filter(|_| desktop).map_or_else(
                    || Role::OnSurfaceVariant.color(dark),
                    |palette| palette.text,
                );
                let x = if !desktop && inputs.direction == Direction::Rtl {
                    label_x(&p, width - inset, 1.)
                } else {
                    label_x(&p, inset, 0.)
                };
                let at = (x, y + (tall - p.height) / 2.0);
                items.push(Item::Text(p, at, ink, *node));
                y += tall;
                continue;
            }
            _ => {
                let line = (0.0, y + rule.1, width, rule.0);
                items.push(Item::Rule(line, Role::OutlineVariant.color(dark)));
                y += rule.0 + 2.0 * rule.1;
                continue;
            }
        };
        y += margin;
        let tall = if desktop {
            (p.height + 9.).max(28.)
        } else {
            (p.height + 16.0).max(MENU_ROW)
        };
        let (ink, tint) = match (row.enabled, row.destructive) {
            (false, _) => (Role::Disabled, Role::Disabled),
            (true, true) => (Role::Error, Role::Error),
            (true, false) => (Role::OnSurface, Role::OnSurfaceVariant),
        };
        let ink = inputs.contrast.filter(|_| desktop).map_or_else(
            || ink.color(dark),
            |p| if row.enabled { p.text } else { p.disabled },
        );
        let tint = inputs.contrast.filter(|_| desktop).map_or_else(
            || tint.color(dark),
            |p| if row.enabled { p.text } else { p.disabled },
        );
        if let Some(role) = lead(row) {
            let x = if !desktop && inputs.direction == Direction::Rtl {
                width - inset - symbol
            } else {
                inset
            };
            let at = (x, y + (tall - symbol) / 2.0);
            let rect = (at.0, at.1, symbol, symbol);
            items.push(Item::Symbol(role, rect, tint));
        }
        let x = inset + indent(row);
        let x = if !desktop && inputs.direction == Direction::Rtl {
            label_x(&p, width - x, 1.)
        } else {
            label_x(&p, x, 0.)
        };
        let at = (x, y + (tall - p.height) / 2.0);
        items.push(Item::Text(p, at, ink, row.node));
        let rect = if desktop {
            (4., y, width - 8., tall)
        } else {
            (0., y, width, tall)
        };
        targets.push(Target {
            rect,
            node: row.node,
            enabled: row.enabled,
            layer: (
                Shape::new(rect, [if desktop { 4. } else { 0. }; 4]),
                Role::OnSurface.color(dark),
            ),
        });
        y += tall + margin;
    }
    // Compose's DropdownMenu scrolls its items within the window's room,
    // as WinUI's MenuFlyout does.
    let height = (y + padding).min(inputs.room.max(4.));
    Layout {
        size: (width, height),
        radius: if desktop { 8. } else { 4. },
        elevation: 3.0,
        container: if desktop {
            Role::Surface.color(dark)
        } else {
            Role::SurfaceContainer.color(dark)
        },
        border: desktop.then(|| Role::OutlineVariant.color(dark)),
        contrast: inputs.contrast.filter(|_| desktop),
        items,
        targets,
        windows: None,
        menu: (height < y + padding)
            .then(|| menu::Scroll::new(y + padding - height, dark, desktop)),
        symbols: if desktop {
            SymbolSet::Fluent
        } else {
            SymbolSet::Material
        },
        symbol_weight: weight(400, inputs.bold_text && !desktop),
    }
}

/// The corner of a menu at `at` that its motion grows from: the one at its
/// anchor, where Compose's `DropdownMenu` puts its transform origin (to the
/// corner). Bit 0: its right side, for a menu that ends at its anchor's
/// end; bit 1: its bottom, for one that opens upward.
pub fn origin(anchor: Rect4, at: (f32, f32), size: (f32, f32)) -> u32 {
    let right = at.0 + size.0 / 2.0 < anchor.0 + anchor.2 / 2.0;
    let bottom = at.1 + size.1 / 2.0 < anchor.1 + anchor.3 / 2.0;
    u32::from(right) | u32::from(bottom) << 1
}

/// Where a menu of `size` goes for an anchor's box, all in the window's
/// points: Compose's `DropdownMenuPositionProvider`. Along the row it
/// starts at the anchor's start, else ends at its end, else sits against
/// the window's far side; down the page it hangs under the anchor, else
/// sits over it, else is centred on its top, else rests at the bottom,
/// never closer than `margins` (top, bottom) to the window's edges.
pub fn place(
    anchor: Rect4,
    size: (f32, f32),
    window: (f32, f32),
    margins: (f32, f32),
    direction: Direction,
) -> (f32, f32) {
    let (w, h) = size;
    let across = if direction == Direction::Rtl {
        [anchor.0 + anchor.2 - w, anchor.0, 0.0]
    } else {
        [anchor.0, anchor.0 + anchor.2 - w, window.0 - w]
    };
    let x = (across.iter().copied())
        .find(|x| *x >= 0.0 && x + w <= window.0)
        .unwrap_or(across[1]);
    let down = [
        (anchor.1 + anchor.3).max(margins.0),
        anchor.1 - h,
        anchor.1 - h / 2.0,
        window.1 - h - margins.1,
    ];
    let y = (down.iter().copied())
        .find(|y| *y >= margins.0 && y + h <= window.1 - margins.1)
        .unwrap_or(down[1]);
    (x.max(0.0), y.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_text_bounds(layout: &Layout) {
        let texts = layout.texts();
        let painted = layout.items.iter().filter_map(|item| match item {
            Item::Text(p, at, _, node) => Some((p, at, node)),
            _ => None,
        });
        for ((node, rect), (p, at, painted_node)) in texts.iter().zip(painted) {
            assert_eq!(node, painted_node);
            let glyphs: Vec<_> = p.layout_runs().flat_map(|r| r.glyphs).collect();
            let left = glyphs
                .iter()
                .map(|g| at.0 + g.x)
                .fold(f32::INFINITY, f32::min);
            let right = glyphs
                .iter()
                .map(|g| at.0 + g.x + g.w)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (rect.0 - left).abs() < 1.,
                "text starts at {left}, accessibility at {rect:?}"
            );
            assert!(
                (rect.0 + rect.2 - right).abs() < 1.,
                "text ends at {right}, accessibility at {rect:?}"
            );
        }
    }

    #[test]
    fn alert_message_joins_adjacent_texts_with_the_same_direction() {
        let mut engine = crate::text::fixtures::engine();
        let inputs = Inputs {
            dark: false,
            direction: Direction::Rtl,
            contrast: None,
            font_scale: 1.,
            bold_text: false,
            window: (600., 800.),
            room: 800.,
        };
        for (message, directions, expected) in [
            (
                "This cannot be undone.\nReally.",
                vec![Direction::Ltr; 2],
                vec!["This cannot be undone. Really."],
            ),
            ("نعم\nلا", vec![Direction::Rtl; 2], vec!["نعم لا"]),
            (
                "First.\nSecond.\nنعم\nلا\nLast.",
                vec![
                    Direction::Ltr,
                    Direction::Ltr,
                    Direction::Rtl,
                    Direction::Rtl,
                    Direction::Ltr,
                ],
                vec!["First. Second.", "نعم لا", "Last."],
            ),
        ] {
            let layout = alert(
                &mut engine,
                (Some("Title"), Some(message)),
                (Direction::Ltr, &directions),
                &[],
                1,
                &inputs,
            );
            let messages: Vec<_> = layout
                .items
                .iter()
                .skip(1)
                .filter_map(|item| match item {
                    Item::Text(p, at, ..) => Some((p, at)),
                    _ => None,
                })
                .collect();
            assert_eq!(messages.len(), expected.len(), "{message}");
            for ((p, _), expected) in messages.iter().zip(expected) {
                let runs: Vec<_> = p.layout_runs().collect();
                assert_eq!(runs.len(), 1, "short joined text fits one line");
                assert_eq!(runs[0].text, expected);
            }
            assert_eq!(
                layout.size.1,
                DIALOG_PADDING * 2.
                    + HEADLINE_SMALL.1
                    + DIALOG_TITLE_GAP
                    + BODY_MEDIUM.1 * messages.len() as f32
                    + DIALOG_TEXT_GAP
            );
        }
    }

    #[test]
    fn rtl_surfaces_keep_each_projected_labels_direction() {
        let source = r#"component App
  state changed = false
  action change
    changed = !changed
  view
    main direction="rtl"
      dialog id="alert" role="alertdialog"
        text "An English title" direction="ltr"
        text "An English message" direction="ltr"
        text "رسالة أخرى"
        button press=change commandfor="alert" command="close"
          text "Beginning of a very long English action ending here" direction="ltr"
        button "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة" press=change commandfor="alert" command="close"
        button "Cancel" commandfor="alert" command="close"
      column id="menu" popover="auto"
        column role="group" aria-label="Beginning of a very long English section ending here" direction="ltr"
          button direction="rtl" press=change
            text "Beginning of a very long English menu item ending here" direction="ltr"
        column role="group" aria-label="بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة"
          button "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة" press=change
      select id="select"
        option "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة" value="two"
        option "Beginning of a very long English option ending here" direction="ltr" value="one"
"#;
        let (p, error) = crate::presenter::Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (300., 600.),
            1.,
            std::path::PathBuf::new(),
            crate::presenter::PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let inputs = Inputs {
            dark: false,
            direction: Direction::Rtl,
            contrast: None,
            font_scale: 1.,
            bold_text: false,
            window: (300., 600.),
            room: 600.,
        };
        let mut truncated = 0;
        let mut prose = 0;
        let mut rtl_truncated = 0;
        for name in ["alert", "menu", "select"] {
            let key = p.host.kernel().find_by_id(name)[0];
            let id = p.host.kernel().node_by_key(key).unwrap().id;
            let plan = if name == "select" {
                p.select_plan(id)
            } else {
                p.surface_plan(id, name == "alert")
            }
            .unwrap();
            let layout = Layout::of(&mut engine, &plan, id, &inputs);
            assert_text_bounds(&layout);
            for item in &layout.items {
                let Item::Text(paragraph, _, _, _) = item else {
                    continue;
                };
                for run in paragraph.layout_runs() {
                    if run.text.starts_with("Beginning") {
                        assert!(!run.rtl, "{name}: authored LTR label");
                        assert!(
                            run.glyphs.iter().any(|g| g.start == 0 && g.end > 0),
                            "{name}: beginning kept"
                        );
                        let ellipsis = run
                            .glyphs
                            .iter()
                            .find(|g| g.start == g.end)
                            .expect("a long label is truncated");
                        assert!(
                            run.glyphs
                                .iter()
                                .filter(|g| g.start != g.end)
                                .all(|g| g.x < ellipsis.x),
                            "{name}: ellipsis at the end"
                        );
                        truncated += 1;
                    } else if run.text.starts_with("بداية") {
                        assert!(run.rtl, "{name}: authored RTL label");
                        let first = run
                            .glyphs
                            .iter()
                            .find(|g| g.start == 0 && g.end > 0)
                            .expect("the first cluster survives truncation");
                        let ellipsis = run
                            .glyphs
                            .iter()
                            .find(|g| g.start == g.end)
                            .expect("a long RTL label is truncated");
                        assert!(
                            run.glyphs.iter().all(|g| g.x <= first.x),
                            "{name}: first cluster at the right"
                        );
                        assert!(
                            run.glyphs
                                .iter()
                                .filter(|g| g.start != g.end)
                                .all(|g| g.x > ellipsis.x),
                            "{name}: ellipsis at the left"
                        );
                        rtl_truncated += 1;
                    } else if run.text.starts_with("An English") {
                        assert!(!run.rtl, "{name}: authored LTR prose");
                        prose += 1;
                    }
                }
            }
        }
        assert_eq!(truncated, 4);
        assert_eq!(rtl_truncated, 4);
        assert_eq!(prose, 2);
    }

    #[test]
    fn truncated_rtl_surface_labels_fit_their_content_boxes() {
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let row = Row {
            node: 2,
            title: "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة".into(),
            direction: Direction::Rtl,
            symbol: Some("checkmark".into()),
            destructive: false,
            enabled: true,
            cancel: true,
            checked: false,
            opens: None,
        };
        for font_scale in [1., 2.] {
            let inputs = Inputs {
                dark: false,
                direction: Direction::Rtl,
                contrast: None,
                font_scale,
                bold_text: false,
                window: (300., 600.),
                room: 600.,
            };
            let entries = [
                Entry::Header(row.title.clone(), 1, Direction::Rtl),
                Entry::Item(row.clone()),
            ];
            let menu = menu(&mut engine, &entries, &inputs, false);
            let alert = alert(
                &mut engine,
                (None, None),
                (Direction::Rtl, &[]),
                std::slice::from_ref(&row),
                0,
                &inputs,
            );
            for (layout, is_alert) in [(&menu, false), (&alert, true)] {
                for item in &layout.items {
                    let Item::Text(p, at, _, node) = item else {
                        continue;
                    };
                    assert!(p
                        .layout_runs()
                        .flat_map(|r| r.glyphs)
                        .any(|g| g.start == g.end));
                    let (left, _, width, _) = text_rect(p, *at);
                    let (start, end) = if is_alert {
                        let (x, _, w, _) = layout.targets[0].rect;
                        assert!(
                            (left + width / 2. - (x + w / 2.)).abs() < 0.01,
                            "alert label must be centered at scale {font_scale}: {left}, {width}, {x}, {w}"
                        );
                        (x + BUTTON_PADDING, x + w - BUTTON_PADDING)
                    } else {
                        let indent = if *node == 2 {
                            MENU_SYMBOL + MENU_INSET
                        } else {
                            0.
                        };
                        (MENU_INSET, layout.size.0 - MENU_INSET - indent)
                    };
                    assert!(left >= start - 0.01, "label starts inside its content box");
                    assert!(
                        left + width <= end + 0.01,
                        "label ends inside its content box at scale {font_scale}: {} > {end}",
                        left + width
                    );
                }
            }
        }
    }

    #[test]
    fn rtl_select_menu_keeps_labels_and_symbols_at_its_widened_start() {
        let mut engine = crate::text::fixtures::engine();
        let entries = vec![
            Entry::Header("Pick".into(), 1, Direction::Rtl),
            Entry::Item(Row {
                node: 2,
                title: "One".into(),
                direction: exact_kernel::Direction::Ltr,
                symbol: None,
                destructive: false,
                enabled: true,
                cancel: false,
                checked: true,
                opens: None,
            }),
            Entry::Separator,
        ];
        let inputs = Inputs {
            dark: false,
            direction: Direction::Rtl,
            contrast: None,
            font_scale: 1.,
            bold_text: false,
            window: (400., 800.),
            room: 800.,
        };
        let mut layout = menu(&mut engine, &entries, &inputs, false);
        assert!(layout.size.0 < 300.);
        layout.widen(300., true);
        for item in &layout.items {
            match item {
                Item::Symbol(_, rect, _) => assert_eq!(rect.0 + rect.2, 288.),
                Item::Text(p, at, _, node) => {
                    let bounds = text_rect(p, *at);
                    let end = if *node == 1 { 288. } else { 252. };
                    assert!((bounds.0 + bounds.2 - end).abs() < 0.01);
                }
                Item::Rule(rect, _) => assert_eq!(rect.2, 300.),
            }
        }
        assert_eq!(layout.targets[0].rect.2, 300.);
        assert_text_bounds(&layout);
    }

    #[test]
    fn rtl_alert_text_aligns_each_line_at_the_inline_start() {
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let inputs = Inputs {
            dark: false,
            direction: Direction::Rtl,
            contrast: None,
            font_scale: 1.,
            bold_text: false,
            window: (400., 800.),
            room: 800.,
        };
        let layout = alert(
            &mut engine,
            (Some("عنوان"), Some("نعم\nلا")),
            (Direction::Rtl, &[Direction::Rtl, Direction::Rtl]),
            &[],
            1,
            &inputs,
        );
        assert_text_bounds(&layout);
        let end = layout.size.0 - DIALOG_PADDING;
        for item in &layout.items {
            if let Item::Text(p, at, ..) = item {
                for run in p.layout_runs() {
                    let right = run.glyphs.iter().map(|g| g.x + g.w).fold(0., f32::max);
                    assert!(
                        (at.0 + right - end).abs() < 1.,
                        "text ends at {}, expected {end}",
                        at.0 + right
                    );
                }
            }
        }
    }

    #[test]
    fn material_surface_paragraphs_measure_with_one_bold_text_step() {
        let row = Row {
            node: 2,
            title: "Action label".into(),
            direction: exact_kernel::Direction::Ltr,
            symbol: Some("calendar".into()),
            destructive: false,
            enabled: true,
            cancel: true,
            checked: false,
            opens: None,
        };
        let alert = Plan::Alert {
            title: Some("Headline".into()),
            title_direction: Direction::Ltr,
            message: Some("Supporting text".into()),
            message_directions: vec![Direction::Ltr],
            rows: vec![row.clone()],
            cancellable: true,
            outside: false,
        };
        let menu = Plan::Menu(vec![
            Entry::Header("Section".into(), 1, Direction::Ltr),
            Entry::Item(row),
        ]);
        let mut engine = crate::text::fixtures::engine();
        for bold_text in [false, true, false] {
            let inputs = Inputs {
                direction: Direction::Ltr,
                dark: false,
                contrast: None,
                font_scale: 1.,
                bold_text,
                window: (400., 800.),
                room: 704.,
            };
            for (plan, weights) in [
                (
                    &alert,
                    if bold_text {
                        vec![600, 600, 700]
                    } else {
                        vec![400, 400, 500]
                    },
                ),
                (
                    &menu,
                    if bold_text {
                        vec![700, 700]
                    } else {
                        vec![500, 500]
                    },
                ),
            ] {
                let layout = Layout::of(&mut engine, plan, 0, &inputs);
                let measured: Vec<_> = layout
                    .items
                    .iter()
                    .filter_map(|item| {
                        let Item::Text(p, _, _, _) = item else {
                            return None;
                        };
                        Some(p.runs()[0].weight)
                    })
                    .collect();
                assert_eq!(measured, weights);
                if matches!(plan, Plan::Menu(_)) {
                    assert_eq!(layout.symbol_weight, if bold_text { 600 } else { 400 });
                }
            }
        }
    }
}
