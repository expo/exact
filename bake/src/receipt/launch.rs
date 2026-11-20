//! The ground the app's first screen stands on, which the iOS launch screen shows until the first
//! frame where the manifest names no launch colour (LLP 1163 D2, LLP 1116.011 D2.3).

use super::literal;
use serde_json::{json, Value};

/// LLP 1116.011 D2.3: the ground the app's first screen stands on, which the
/// iOS launch screen shows until its first frame: `"grouped"` where it is
/// the metric the compiler gives a route that scrolls a grouped list
/// (`-exact-grouped-page-ground`, LLP 1116.007 D5), iOS's grouped grey, or
/// that role written (`-exact-grouped-background`), `"system"` for any other
/// (the platform's page, `systemBackground`). The plan does not say which
/// route the app opens on, so every one of [`screens`] must be grouped. What
/// may paint over a grouped ground leaves it grouped, as either answer is a
/// platform page. A ground that names a colour is [`launch_ground`]'s.
pub(super) fn launch_background(plan: &exact_plan::Plan) -> &'static str {
    let screens = screens(plan);
    let grouped = |screen: &Screen| {
        matches!(
            screen.written,
            Some("-exact-grouped-page-ground" | "-exact-grouped-background")
        )
    };
    if !screens.is_empty() && screens.iter().all(grouped) {
        "grouped"
    } else {
        "system"
    }
}

/// LLP 1163 D2: the app's own ground, light and dark, which the iOS launch
/// screen shows in a scheme the manifest names no launch colour for: the
/// ground every screen the app can open on stands on ([`screens`]), a colour
/// written as such (in Display P3 beyond sRGB, [`wide_ground`]) or a theme's
/// name for one, on one side in both schemes
/// where that screen or the root fixes a `color-scheme`. The plan does not
/// say which route the app opens on, so the screens must agree. Null, and
/// the launch screen the platform's page, wherever the first frame may show
/// another colour: screens that disagree; a role or a platform colour,
/// which the host draws as the OS or the app's tint has it; a colour beyond
/// Display P3; a theme's colour that varies by an app mode; a computed,
/// translucent or pictured ground; a computed `color-scheme`; a ground
/// something may paint over ([`covered`]); and an app with a tab panel,
/// whose routes may stand on the panel or a box above it. Boxes sized in
/// points can still cover the ground, one larger than the screen or several
/// written one after another that fill it; the bake reads
/// the ground the app opens on in the system's scheme, not one it picks at
/// run time (`setScheme`), and an update that changes it reaches the launch
/// screen only with a new binary.
pub(super) fn launch_ground(plan: &exact_plan::Plan) -> Value {
    use exact_kernel::PropId;
    let panel = plan.nodes.iter().any(|node| {
        matches!(
            literal(
                plan,
                &node,
                exact_plan::BindingKind::Prop,
                PropId::AccessibilityRole as u16
            ),
            Some(None | Some("tabpanel"))
        )
    });
    if panel {
        return Value::Null;
    }
    // Build the theme table for this plan, never another's whose buffers this one's reuse
    exact_runner::bridge::role_overrides(plan);
    let table = (!plan.theme.is_empty()).then(|| exact_runner::bridge::theme_table(plan));
    let _theme = table.as_ref().map(exact_kernel::style::theme::bind);
    let mut grounds = screens(plan).into_iter().map(|screen| {
        if covered(plan, &screen) {
            return Value::Null;
        }
        let node = plan.node(exact_plan::NodesId(screen.ground()));
        ground(
            plan,
            &node,
            screen.written,
            table.as_deref().unwrap_or_default(),
        )
    });
    let first = grounds.next().unwrap_or(Value::Null);
    if grounds.all(|g| g == first) {
        first
    } else {
        Value::Null
    }
}

/// One screen's ground as [`launch_ground`] reads it, while the app's theme
/// (`theme`) is bound. A theme's colour that varies by an app mode is the
/// default mode's here, and a `-exact-mode` pin or a mode the app sets may
/// show another.
fn ground(
    plan: &exact_plan::Plan,
    node: &exact_plan::NodesRow,
    written: Option<&str>,
    theme: &[exact_kernel::style::theme::Entry],
) -> Value {
    use exact_kernel::style::{theme::recorded, Color, ColorValue};
    let (value, read) = recorded(|| {
        written.and_then(|text| {
            ColorValue::parse_light_dark(text).or_else(|| Color::parse(text).map(ColorValue::Fixed))
        })
    });
    let varies = read
        .iter()
        .any(|name| theme.iter().any(|e| e.name == *name && e.varies));
    let (
        Some(value @ (ColorValue::Fixed(_) | ColorValue::LightDark(..) | ColorValue::Wide(_))),
        false,
    ) = (value, varies)
    else {
        return Value::Null;
    };
    let scheme = |node: &exact_plan::NodesRow| {
        literal(
            plan,
            node,
            exact_plan::BindingKind::Style,
            exact_kernel::StyleId::ColorScheme as u16,
        )
    };
    let (light, dark) =
        match scheme(node).or_else(|| plan.nodes.iter().next().and_then(|root| scheme(&root))) {
            None => (false, true),
            Some(Some("dark")) => (true, true),
            Some(Some("light")) => (false, false),
            Some(_) => return Value::Null,
        };
    // An opaque colour only: a translucent ground is not what the first frame shows
    let hex = |dark| match value {
        ColorValue::Wide(id) => wide_ground(exact_kernel::style::wide::wide(id)?.half(dark)),
        _ => {
            let Color(rgba) = value.resolve(dark);
            (rgba & 0xff == 0xff).then(|| format!("#{:06x}", rgba >> 8))
        }
    };
    match (hex(light), hex(dark)) {
        (Some(light), Some(dark)) => json!({"light": light, "dark": dark}),
        _ => Value::Null,
    }
}

/// A colour in its own space (LLP 1100 D2) as the launch screen can show it,
/// which the host draws in that space: as hex within sRGB, else as
/// `color(display-p3 r g b)` within Display P3. None for a translucent colour
/// or one beyond Display P3, which no launch colour set holds.
fn wide_ground(colour: exact_color::Wide) -> Option<String> {
    if colour.alpha < 1.0 {
        return None;
    }
    if colour.in_srgb() {
        let c = colour.srgb8();
        return Some(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
    }
    let p3 = exact_color::MixSpace::DisplayP3.from_linear_srgb(colour.linear_srgb());
    p3.iter()
        .all(|c| (-1e-4..=1.0 + 1e-4).contains(c))
        .then(|| {
            let [r, g, b] = p3.map(|c| c.clamp(0.0, 1.0));
            format!("color(display-p3 {r:.4} {g:.4} {b:.4})")
        })
}

/// A screen the app can open on ([`screens`]): its route (the app root in an
/// app without routes), whether it stands on the app root's
/// `background-color` rather than its own, and that colour as written
/// (`None` where it is computed, unwritten or under a `background-image`).
struct Screen<'p> {
    route: u32,
    on_root: bool,
    written: Option<&'p str>,
}

impl Screen<'_> {
    /// The node whose `background-color` is the screen's ground
    fn ground(&self) -> u32 {
        if self.on_root {
            0
        } else {
            self.route
        }
    }
}

/// The screens the app can open on, in the plan's order: each route
/// ([`is_route`]), standing on its own `background-color` where it writes
/// one, else the app root's (the plan's first node, the root of the
/// component the app starts from), or the root alone in an app without
/// routes. A route counts past the page the compiler writes for an unknown
/// address (its test id `not-found`), and not as a sheet or a full-screen
/// cover, which a `navigationPresentation` written other than `""` shows over
/// another screen; one whose presentation is computed is a sheet, never
/// first, or a page on its own ground. The compiler places a route only in
/// the navigation root, the app root, or in a tab panel.
fn screens(plan: &exact_plan::Plan) -> Vec<Screen<'_>> {
    use exact_kernel::PropId;
    let Some(root) = plan.nodes.iter().next() else {
        return Vec::new();
    };
    let pictured = |node: &exact_plan::NodesRow| {
        literal(
            plan,
            node,
            exact_plan::BindingKind::Style,
            exact_kernel::StyleId::BackgroundImage as u16,
        )
        .is_some()
    };
    let on_root = background(plan, &root)
        .flatten()
        .filter(|_| !pictured(&root));
    let mut screens: Vec<_> = plan
        .nodes
        .iter()
        .zip(0..)
        .filter_map(|(node, route)| {
            let prop = |id: PropId| literal(plan, &node, exact_plan::BindingKind::Prop, id as u16);
            if !is_route(plan, &node)
                || prop(PropId::TestId) == Some(Some("not-found"))
                || matches!(prop(PropId::NavigationPresentation), Some(Some(shown)) if !shown.is_empty())
            {
                return None;
            }
            let own = |written| Screen { route, on_root: false, written };
            Some(match background(plan, &node) {
                _ if pictured(&node) => own(None),
                Some(written) => own(written),
                None => Screen { route, on_root: true, written: on_root },
            })
        })
        .collect();
    if screens.is_empty() {
        screens.push(Screen {
            route: 0,
            on_root: true,
            written: on_root,
        });
    }
    screens
}

/// Whether `node` is a route: a `navigationKey` with no `navigationBack`,
/// which a stack's navigation root adds (a tab root is the app root)
fn is_route(plan: &exact_plan::Plan, node: &exact_plan::NodesRow) -> bool {
    use exact_kernel::PropId;
    let prop = |id: PropId| literal(plan, node, exact_plan::BindingKind::Prop, id as u16);
    prop(PropId::NavigationKey).is_some() && prop(PropId::NavigationBack).is_none()
}

/// Whether something may paint over `screen`'s ground in its first frame out
/// to the screen's edges: an effect ([`effect`]) on the ground's node or a box
/// it stands in, or a node of the screen that paints ([`paints`]) where it and
/// every box between it and the screen may fill their parents ([`fills`]). A
/// screen's nodes are its route's and the app root's outside every route,
/// which every screen shows.
fn covered(plan: &exact_plan::Plan, screen: &Screen) -> bool {
    use exact_plan::NodesId;
    let ground = NodesId(screen.ground());
    if std::iter::once(ground)
        .chain(ancestors(plan, ground))
        .any(|n| effect(plan, &plan.node(n)))
    {
        return true;
    }
    plan.nodes.iter().zip(0..).any(|(node, at)| {
        if at == 0 || at == screen.route || is_route(plan, &node) {
            return false;
        }
        let up = ancestors(plan, NodesId(at));
        // The boxes it stands in up to its screen: its route, or the app root
        let boxes = match up.iter().position(|n| is_route(plan, &plan.node(*n))) {
            Some(i) if up[i].0 == screen.route => &up[..=i],
            Some(_) => return false,
            None => &up[..],
        };
        let between = &boxes[..boxes.len().saturating_sub(1)];
        paints(plan, &node, between, screen.written) && fills(plan, NodesId(at), boxes)
    })
}

/// Whether `node` carries an effect that changes what it and its content
/// paint: an animation, a filter, a mask, a clip, a blend mode, an opacity
/// other than 1, or the node hidden or not displayed
fn effect(plan: &exact_plan::Plan, node: &exact_plan::NodesRow) -> bool {
    use exact_kernel::StyleId;
    let style = |id: StyleId| literal(plan, node, exact_plan::BindingKind::Style, id as u16);
    [
        (StyleId::Animation, "none"),
        (StyleId::Filter, "none"),
        (StyleId::MaskImage, "none"),
        (StyleId::ClipPath, "none"),
        (StyleId::MixBlendMode, "normal"),
        (StyleId::Visibility, "visible"),
    ]
    .into_iter()
    .any(|(id, unsaid)| style(id).is_some_and(|written| written != Some(unsaid)))
        || style(StyleId::Opacity).is_some()
            && number(
                plan,
                node,
                exact_plan::BindingKind::Style,
                StyleId::Opacity as u16,
            ) != Some(1.0)
        || matches!(style(StyleId::Display), Some(Some("none") | None))
}

/// Whether `node` paints something of its own: a picture, a material, an
/// image, canvas, video, SVG, web or native view, a list the host draws (any
/// but `appearance="none"`), or a colour other than `transparent`. A colour
/// written as the screen's ground (`ground`) is that ground, unless the node
/// carries an effect or it or a box between it and the screen (`between`)
/// writes a `color-scheme`.
fn paints(
    plan: &exact_plan::Plan,
    node: &exact_plan::NodesRow,
    between: &[exact_plan::NodesId],
    ground: Option<&str>,
) -> bool {
    use exact_kernel::{NodeType, StyleId};
    let style = |node: &exact_plan::NodesRow, id: StyleId| {
        literal(plan, node, exact_plan::BindingKind::Style, id as u16)
    };
    let said = |id: StyleId| style(node, id).is_some_and(|written| written != Some("none"));
    let drawn = match NodeType::from_wire(node.node_type) {
        Some(
            NodeType::Image
            | NodeType::Canvas
            | NodeType::Video
            | NodeType::Svg
            | NodeType::WebView
            | NodeType::NativeView,
        ) => true,
        Some(NodeType::List) => style(node, StyleId::Appearance) != Some(Some("none")),
        _ => false,
    };
    let colour = style(node, StyleId::BackgroundColor);
    let same = colour == Some(ground)
        && !effect(plan, node)
        && std::iter::once(node.clone())
            .chain(between.iter().map(|n| plan.node(*n)))
            .all(|n| style(&n, StyleId::ColorScheme).is_none());
    drawn
        || said(StyleId::BackgroundImage)
        || literal(
            plan,
            node,
            exact_plan::BindingKind::Prop,
            exact_kernel::PropId::BackgroundMaterial as u16,
        )
        .is_some_and(|written| written != Some("none"))
        || colour.is_some_and(|c| c != Some("transparent")) && !same
}

/// Whether `node` may fill the screen whose node is the last of `boxes`, the
/// boxes it stands in, nearest first: it and each box between may fill
/// their parents ([`fills_parent`]), an absolutely positioned one filling its
/// nearest positioned box or the screen where it reaches its edges or is as
/// large as it on each axis.
fn fills(
    plan: &exact_plan::Plan,
    node: exact_plan::NodesId,
    boxes: &[exact_plan::NodesId],
) -> bool {
    use exact_kernel::StyleId;
    let style = |node: &exact_plan::NodesRow, id: StyleId| {
        literal(plan, node, exact_plan::BindingKind::Style, id as u16)
    };
    let (mut at, mut next) = (node, 0);
    while next < boxes.len() {
        let row = plan.node(at);
        if !matches!(
            style(&row, StyleId::PositionType),
            Some(Some("absolute") | None)
        ) {
            if !fills_parent(plan, &row, &plan.node(boxes[next])) {
                return false;
            }
            (at, next) = (boxes[next], next + 1);
            continue;
        }
        let edge = |id: StyleId| style(&row, id).is_some();
        let across =
            edge(StyleId::Left) && edge(StyleId::Right) || full(plan, &row, StyleId::Width);
        let down = edge(StyleId::Top) && edge(StyleId::Bottom) || full(plan, &row, StyleId::Height);
        if !(across && down) {
            return false;
        }
        let positioned = |n: &exact_plan::NodesId| {
            style(&plan.node(*n), StyleId::PositionType).is_some_and(|p| p != Some("static"))
        };
        let holder = boxes[next..boxes.len() - 1]
            .iter()
            .position(positioned)
            .map_or(boxes.len() - 1, |i| next + i);
        (at, next) = (boxes[holder], holder + 1);
    }
    true
}

/// Whether an in-flow `node` may fill `parent` on both axes. On each axis a
/// size or minimum that fills ([`full`]) does, and so does a flex
/// container's main axis where the node grows. With no size that bounds it,
/// so do a block's width, a grid's cell, a flex container's cross axis it
/// stretches on, and a scroll's or a list's content downwards, as do the
/// rows a `for` repeats there whatever each one's height. A parent padded on
/// both sides, which the node's margins do not undo, and margins of the
/// node's own on both sides bound it across.
fn fills_parent(
    plan: &exact_plan::Plan,
    node: &exact_plan::NodesRow,
    parent: &exact_plan::NodesRow,
) -> bool {
    use exact_kernel::{NodeType, StyleId};
    use exact_plan::{BindingKind::Style, NodesRow};
    let style = |node: &NodesRow, id: StyleId| literal(plan, node, Style, id as u16);
    let number = |node: &NodesRow, id: StyleId| number(plan, node, Style, id as u16);
    let unbounded = |id: StyleId| matches!(style(node, id), None | Some(Some("auto")));
    let display = style(parent, StyleId::Display);
    let flex = matches!(display, Some(Some("flex" | "inline-flex") | None));
    let grid = matches!(display, Some(Some("grid" | "inline-grid") | None));
    let direction = style(parent, StyleId::FlexDirection);
    let column = flex && matches!(direction, Some(Some("column" | "column-reverse") | None));
    let row = flex && !matches!(direction, Some(Some("column" | "column-reverse")));
    let aligned = |written: Option<Option<&str>>| matches!(written, Some(Some(align)) if align != "stretch" && align != "normal");
    let stretches =
        !aligned(style(parent, StyleId::AlignItems)) && !aligned(style(node, StyleId::AlignSelf));
    let grows = style(node, StyleId::FlexGrow).is_some()
        && number(node, StyleId::FlexGrow) != Some(0.0)
        || full(plan, node, StyleId::FlexBasis);
    let content = matches!(
        NodeType::from_wire(parent.node_type),
        Some(NodeType::ScrollView | NodeType::List)
    );
    let repeated = node
        .arm
        .is_some_and(|a| plan.region(plan.arm(a).region).kind == exact_plan::RegionKind::Each);
    // A length in points, or one in px, rem or em; a computed or `calc()` one may be 0
    let length = |node: &NodesRow, id: StyleId| {
        number(node, id).is_some_and(|n| n > 0.0)
            || matches!(style(node, id), Some(Some(written)) if positive_length(written))
    };
    let undone = |id: StyleId| {
        number(node, id).is_some_and(|m| m < 0.0)
            || matches!(style(node, id), Some(Some(m)) if m.starts_with('-'))
            || style(node, id) == Some(None) && number(node, id).is_none()
    };
    let padded = length(parent, StyleId::PaddingLeft)
        && length(parent, StyleId::PaddingRight)
        && !undone(StyleId::MarginLeft)
        && !undone(StyleId::MarginRight);
    let inset = length(node, StyleId::MarginLeft) && length(node, StyleId::MarginRight);
    let across = full(plan, node, StyleId::Width)
        || full(plan, node, StyleId::MinWidth)
        || row && grows
        || unbounded(StyleId::Width) && (!flex || column && stretches);
    let down = full(plan, node, StyleId::Height)
        || full(plan, node, StyleId::MinHeight)
        || column && grows
        || content && (repeated || unbounded(StyleId::Height))
        || unbounded(StyleId::Height) && (grid || row && stretches);
    across && down && !padded && !inset
}

/// Whether a node's size `id` may be its parent's or the screen's: `100%` or
/// more, 100 viewport units or more, an expression such as `calc()`, or
/// computed (a number is points)
fn full(plan: &exact_plan::Plan, node: &exact_plan::NodesRow, id: exact_kernel::StyleId) -> bool {
    match literal(plan, node, exact_plan::BindingKind::Style, id as u16) {
        Some(Some(written)) if written.contains('(') => true,
        Some(Some(written)) => {
            let (value, unit) = split_length(written);
            value >= 100.0
                && matches!(
                    unit,
                    "%" | "vw"
                        | "vh"
                        | "dvw"
                        | "dvh"
                        | "svw"
                        | "svh"
                        | "lvw"
                        | "lvh"
                        | "vmin"
                        | "vmax"
                )
        }
        Some(None) => number(plan, node, exact_plan::BindingKind::Style, id as u16).is_none(),
        None => false,
    }
}

/// Whether a length written as text is more than 0 in px, rem or em
fn positive_length(written: &str) -> bool {
    let (value, unit) = split_length(written);
    value > 0.0 && matches!(unit, "px" | "rem" | "em")
}

/// A length written as text, as its number (0 where there is none) and unit
fn split_length(written: &str) -> (f64, &str) {
    let unit = written.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    let value = written[..written.len() - unit.len()].parse().unwrap_or(0.0);
    (value, unit)
}

/// The nodes `node` stands in, nearest first, through the regions (`if`,
/// `for`) between them
fn ancestors(plan: &exact_plan::Plan, node: exact_plan::NodesId) -> Vec<exact_plan::NodesId> {
    let row = plan.node(node);
    let (mut parent, mut arm, mut up) = (row.parent, row.arm, Vec::new());
    loop {
        if let Some(n) = parent.take() {
            up.push(n);
            let row = plan.node(n);
            (parent, arm) = (row.parent, row.arm);
            continue;
        }
        let Some(a) = arm.take() else {
            return up;
        };
        let region = plan.region(plan.arm(a).region);
        (parent, arm) = (region.parent, region.arm);
    }
}

/// A node's `background-color`: none where it writes none, `Some(None)`
/// where its value is computed.
fn background<'p>(
    plan: &'p exact_plan::Plan,
    node: &exact_plan::NodesRow,
) -> Option<Option<&'p str>> {
    literal(
        plan,
        node,
        exact_plan::BindingKind::Style,
        exact_kernel::StyleId::BackgroundColor as u16,
    )
}

/// A node's binding written as a number: none where it is not one or not written
fn number(
    plan: &exact_plan::Plan,
    node: &exact_plan::NodesRow,
    kind: exact_plan::BindingKind,
    id: u16,
) -> Option<f64> {
    node.bindings.iter().find_map(|b| {
        let b = plan.binding(b);
        let code = plan
            .code
            .get(b.expr.offset as usize..(b.expr.offset + b.expr.len) as usize)?;
        // `Number` and its operand, then `Return`
        let constant = code.len() == 10
            && code[0] == exact_plan::Opcode::Number as u8
            && code[9] == exact_plan::Opcode::Return as u8;
        (b.kind == kind && b.id == id && constant)
            .then(|| f64::from_le_bytes(code[1..9].try_into().unwrap()))
    })
}
