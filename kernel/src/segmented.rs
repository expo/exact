//! A content `tablist` or `radiogroup` as the platform's segmented control on the
//! painted hosts, as iOS shows a `UISegmentedControl` for the same group
//! (LLP 1116.004 D5; docs/contract-for-agents.md, `role="radiogroup"`).
//! Under the Material look (Android) it is Material 3's segmented buttons
//! (LLP 1115.001 D11); under the neutral look (the Linux desktop, Windows)
//! the web's neutral segmented control, `host/web/neutral.css`'s track with
//! the checked radio raised on it (LLP 1115.000 D17).
//!
//! Which group is one is iOS's rule (`SegmentsIOS.sync`): horizontal,
//! no written `appearance: none`, each live child a native button with
//! the group's member role (`tab` or `radio`). Material asks for two or
//! more, each one text or one symbol. Navigation-root tabs keep their bar.
//! The neutral look follows the web's sheet: `bordered` or `gray` members,
//! and tablists with `aria-controls` keep their tab-bar treatment.
//!
//! Layout is here, paint in the host. Everything this sets is a default
//! beneath what the author wrote: a group whose author wrote its layout (a
//! `display`, a flex or grid row) keeps it, and each other row this writes
//! stays the author's where they wrote it. What it reads (the group, its
//! parent, its children and what they hold) restyles the group and its
//! radios when it changes ([`prop_changed`], [`style_changed`],
//! [`children_changed`]); a segment's place is read when it is asked for.
use crate::arena::NodeArena;
use crate::generated::{AlignItems, AlignSelf, Display, FlexDirection};
use crate::style::metrics::{self, Column};
use crate::{ControlKind, PropId, StyleId};
use taffy::style::{LengthPercentage as Lp, LengthPercentageAuto as Lpa};

/// Where a segment sits in its group, in reading order: which of its ends
/// are the group's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentPlace {
    /// The first of several: its start is the group's.
    First,
    /// Between two others.
    Middle,
    /// The last of several: its end is the group's.
    Last,
    /// The only one.
    Only,
}

impl NodeArena {
    /// Whether `slot` is a member a segment can stand for under `column`.
    fn segment_member(&self, slot: u32, column: Column, role: &str) -> bool {
        if ControlKind::of(self.node_type(slot), self.props(slot)) != Some(ControlKind::Button)
            || self.props(slot).str(PropId::AccessibilityRole) != Some(role)
        {
            return false;
        }
        match column {
            // One text or one symbol (`SegmentFace` on iOS).
            Column::Material => self.press_face(slot).is_some_and(|face| {
                face.fits
                    && face.subtitle.is_none()
                    && (face.title.is_some() != (face.symbol.is_some() || face.raster))
            }),
            _ => matches!(
                self.props(slot)
                    .str(PropId::ButtonStyle)
                    .unwrap_or("bordered"),
                "bordered" | "gray"
            ),
        }
    }

    /// The members of a segmented tablist or radio group, in order.
    pub(crate) fn segmented_members(&self, slot: u32) -> Option<Vec<u32>> {
        let column = metrics::column();
        let props = self.props(slot);
        if !matches!(column, Column::Material | Column::Neutral)
            || !matches!(
                props.str(PropId::AccessibilityRole),
                Some("radiogroup" | "tablist")
            )
            || props.str(PropId::AccessibilityOrientation) == Some("vertical")
        {
            return None;
        }
        let tablist = props.str(PropId::AccessibilityRole) == Some("tablist");
        if tablist {
            if column == Column::Neutral
                && self
                    .children(slot)
                    .iter()
                    .any(|c| self.props(*c).get(PropId::AccessibilityControls).is_some())
            {
                return None;
            }
            // A navigation root's own list is its navigation bar. Local panel
            // controls inside a route remain tabs of that route.
            let mut ancestor = self.parent(slot);
            let mut root = None;
            while let Some(at) = ancestor {
                if self.props(at).str(PropId::NavigationKey).is_some() {
                    root = Some(at);
                }
                ancestor = self.parent(at);
            }
            if root
                .and_then(|root| self.root_tabs(root))
                .is_some_and(|tabs| tabs.tablist == self.local_id(slot))
            {
                return None;
            }
        }
        let style = self.style(slot);
        if style.written.has(StyleId::Appearance) && style.appearance == crate::Appearance::None {
            return None;
        }
        // A child that `display: none` takes out of the layout is no segment.
        let radios: Vec<u32> = (self.children(slot).iter().copied())
            .filter(|c| self.is_live(*c) && self.style(*c).display != Display::None)
            .collect();
        let least = if column == Column::Material { 2 } else { 1 };
        (radios.len() >= least
            && radios
                .iter()
                .all(|r| self.segment_member(*r, column, if tablist { "tab" } else { "radio" })))
        .then_some(radios)
    }

    /// Whether `slot` is itself a grouped list's row: as the web's sheet has
    /// it, such a group spans the card, with no track of its own, so the
    /// row's separator still reaches the card's edge.
    pub(crate) fn grouped_row(&self, slot: u32) -> bool {
        self.props(slot).bool(PropId::GroupedRowSeparator) == Some(true)
    }

    /// Whether the author laid `slot` out, which the equal columns would
    /// replace: wrote its `display` or a flex or grid row of its own, or
    /// sized one of its children along the row (a `flex` row, a width).
    pub(crate) fn authored_layout(&self, slot: u32) -> bool {
        let wrote = |s: u32, rows: &[StyleId]| {
            let written = self.style(s).written;
            rows.iter().any(|id| written.has(*id))
        };
        wrote(
            slot,
            &[
                StyleId::Display,
                StyleId::FlexDirection,
                StyleId::FlexWrap,
                StyleId::JustifyContent,
                StyleId::GridTemplateColumns,
                StyleId::GridAutoFlow,
            ],
        ) || (self.children(slot).iter()).any(|c| {
            // The children `segmented_members` counts: a hidden one sizes
            // nothing.
            self.is_live(*c)
                && self.style(*c).display != Display::None
                && wrote(
                    *c,
                    &[
                        StyleId::FlexBasis,
                        StyleId::FlexGrow,
                        StyleId::FlexShrink,
                        StyleId::Width,
                        StyleId::MinWidth,
                        StyleId::MaxWidth,
                    ],
                )
        })
    }

    /// Where `slot` sits among its group's segments, when it is one, in the
    /// order the group lays them out along its main axis: backwards in a
    /// reversed flex direction (right to left is the painter's and the
    /// margins' own mirroring). A radio's own `order` would move it too, but
    /// a button that writes one is no native button (LLP 1104 D2).
    pub(crate) fn segment_place(&self, slot: u32) -> Option<SegmentPlace> {
        if !matches!(metrics::column(), Column::Material | Column::Neutral) {
            return None;
        }
        let group = self.parent(slot)?;
        let mut radios = self.segmented_members(group)?;
        let style = self.effective_style(group);
        if style.display == Display::Flex
            && matches!(
                style.flex_direction,
                FlexDirection::RowReverse | FlexDirection::ColumnReverse
            )
        {
            radios.reverse();
        }
        let at = radios.iter().position(|r| *r == slot)?;
        Some(match (at, radios.len()) {
            (_, 1) => SegmentPlace::Only,
            (0, _) => SegmentPlace::First,
            (at, n) if at + 1 == n => SegmentPlace::Last,
            _ => SegmentPlace::Middle,
        })
    }
}

/// A length of `column`'s, px.
fn metric(index: usize, column: Column) -> f32 {
    metrics::length_in(index, column, 0.0, 0.0).unwrap_or(0.0)
}

/// The engine style of a segmented group and of its segments.
///
/// The group is as wide as its segments (the web's `width: fit-content`,
/// Compose's `width(IntrinsicSize.Min)`) and no wider than what holds it.
/// Under Material it is one row of equal columns (`Modifier.weight(1f)` on
/// each segment), so it is as wide as its widest segment makes them all;
/// each segment after the first reaches back over the one before by the
/// outline, so neighbours share one line (`Arrangement.spacedBy(-BorderWidth)`).
/// Under the neutral look each segment keeps its own width.
pub(crate) fn apply(arena: &NodeArena, slot: u32, s: &mut taffy::style::Style) {
    let column = metrics::column();
    if !matches!(column, Column::Material | Column::Neutral) {
        return;
    }
    let written = arena.style(slot).written;
    if arena.segmented_members(slot).is_some() {
        // The track's padding is inside the group's width, as the web's
        // sheet has it (`box-sizing: border-box`).
        if !written.has(StyleId::BoxSizing) {
            s.box_sizing = taffy::style::BoxSizing::BorderBox;
        }
        if !arena.grouped_row(slot) {
            hug(arena, slot, s);
        }
        if column == Column::Material && !arena.authored_layout(slot) {
            s.display = taffy::style::Display::Grid;
            s.grid_auto_flow = taffy::style::GridAutoFlow::Column;
            // `minmax(0, 1fr)`: equal shares however narrow the group, as
            // Compose's weights share the row; a bare `1fr` keeps each
            // segment's own width as its minimum and overflows.
            s.rare.grid_auto_columns = vec![taffy::style_helpers::minmax(
                taffy::style::MinTrackSizingFunction::length(0.0),
                taffy::style::MaxTrackSizingFunction::fr(1.0),
            )];
            if !written.has(StyleId::ColumnGap) {
                s.gap.width = Lp::length(0.0);
            }
        }
        return;
    }
    let Some(place) = arena.segment_place(slot) else {
        return;
    };
    if column == Column::Neutral {
        return;
    }
    let laid_out = arena
        .parent(slot)
        .is_some_and(|g| !arena.authored_layout(g));
    // The start side; `sides::mirror_unsaid` moves it to the right under
    // right to left, as it moves any margin the author did not write.
    if laid_out
        && matches!(place, SegmentPlace::Middle | SegmentPlace::Last)
        && !written.has(StyleId::MarginLeft)
        && !written.has(StyleId::MarginRight)
    {
        s.margin.left = Lpa::length(-metric(
            crate::generated::metric::SEGMENTED_BUTTON_OUTLINE,
            Column::Material,
        ));
    }
}

/// `width: max-content; max-width: 100%`, each where the author wrote none:
/// as wide as its segments, in a block, a flex column or a grid alike (an
/// automatic width would stretch there), and no wider than what holds it.
/// In a column that would stretch it, its own width at the start.
fn hug(arena: &NodeArena, slot: u32, s: &mut taffy::style::Style) {
    let own = arena.style(slot);
    if !own.written.has(StyleId::MaxWidth) {
        s.max_size.width = taffy::Dimension::percent(1.0);
    }
    if own.written.has(StyleId::Width) {
        return;
    }
    s.size.width = taffy::style::Dimension::max_content();
    s.item_is_table = true;
    let Some(parent) = arena.parent(slot) else {
        return;
    };
    let holder = arena.effective_style(parent);
    let column = holder.display == Display::Flex
        && matches!(
            holder.flex_direction,
            FlexDirection::Column | FlexDirection::ColumnReverse
        );
    if column
        && own.align_self == AlignSelf::Auto
        && matches!(holder.align_items, AlignItems::Normal | AlignItems::Stretch)
    {
        s.align_self = Some(taffy::style::AlignSelf::START);
    }
}

/// What changed on a node, for [`changed`].
enum Change {
    Prop(PropId),
    Style(crate::StyleMask),
}

/// [`changed`] for a prop.
pub(crate) fn prop_changed(
    arena: &mut NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    slot: u32,
    prop: PropId,
    touched: &mut Vec<crate::NodeKey>,
) -> bool {
    changed(arena, layout, slot, Change::Prop(prop), touched)
}

/// [`changed`] for style rows.
pub(crate) fn style_changed(
    arena: &mut NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    slot: u32,
    mask: crate::StyleMask,
    touched: &mut Vec<crate::NodeKey>,
) -> bool {
    changed(arena, layout, slot, Change::Style(mask), touched)
}

/// Restyle each group whose layout reads `slot`, and every radio of it,
/// when something it reads changes there:
/// - on the group or a radio: a role, an orientation, a button style, a
///   `type`, the grouped-row mark, an `appearance` or any layout row (its
///   `display`, its flex and grid rows, a radio's `order`, its sizes);
/// - inside a radio: a text's words or an image's source, which decide
///   whether its face is one a segment can show;
/// - on the group's parent: its `display`, `flex-direction` and
///   `align-items`, which decide how the group hugs its segments.
///
/// Whether any group was.
fn changed(
    arena: &mut NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    slot: u32,
    what: Change,
    touched: &mut Vec<crate::NodeKey>,
) -> bool {
    if !matches!(metrics::column(), Column::Material | Column::Neutral) {
        return false;
    }
    let group = |s: u32| {
        matches!(
            arena.props(s).str(PropId::AccessibilityRole),
            Some("radiogroup" | "tablist")
        )
    };
    let parent = arena.parent(slot);
    let mut groups: Vec<u32> = Vec::new();
    match what {
        Change::Prop(
            PropId::AccessibilityRole
            | PropId::AccessibilityControls
            | PropId::AccessibilityOrientation
            | PropId::ButtonStyle
            | PropId::Type
            | PropId::GroupedRowSeparator,
        ) => {
            // A role that changes may have been a group's: its children
            // are read again whatever it is now.
            let role = matches!(what, Change::Prop(PropId::AccessibilityRole));
            groups.extend(std::iter::once(slot).filter(|s| role || group(*s)));
            groups.extend(parent.filter(|p| group(*p)));
        }
        Change::Prop(PropId::Text | PropId::ImageSource) => {
            groups.extend(parent.and_then(|p| arena.parent(p)).filter(|g| group(*g)));
        }
        Change::Prop(PropId::NavigationKey) => {
            // Root-tab discovery stops at route keys. A new key can hide
            // formerly root-owned tabs, whose cached layout must change too.
            groups.extend(
                arena
                    .subtree(slot)
                    .into_iter()
                    .filter(|s| arena.props(*s).str(PropId::AccessibilityRole) == Some("tablist")),
            );
        }
        Change::Prop(_) => {}
        Change::Style(mask) => {
            if mask.intersects(crate::StyleMask::LAYOUT) || mask.has(StyleId::Appearance) {
                groups.extend(std::iter::once(slot).filter(|s| group(*s)));
                groups.extend(parent.filter(|p| group(*p)));
            }
            if [
                StyleId::Display,
                StyleId::FlexDirection,
                StyleId::AlignItems,
            ]
            .iter()
            .any(|id| mask.has(*id))
            {
                let held = arena.children(slot).iter().copied().filter(|c| group(*c));
                groups.extend(held);
            }
        }
    }
    if matches!(
        what,
        Change::Prop(
            PropId::Id
                | PropId::NavigationKey
                | PropId::AccessibilityRole
                | PropId::AccessibilityControls
        )
    ) {
        groups.extend(navigation_lists(arena, slot));
    }
    groups.sort_unstable();
    groups.dedup();
    for group in &groups {
        let members: Vec<u32> = std::iter::once(*group)
            .chain(arena.children(*group).iter().copied())
            .collect();
        for s in members {
            if let Some(node) = arena.taffy(s) {
                layout.restyle(arena, s, node);
                layout.mark_dirty(node);
            }
            arena.mark_export_dirty(s, crate::NodeFlags::PAINT_DIRTY);
            touched.push(arena.key(s));
        }
    }
    !groups.is_empty()
}

/// [`changed`] for a change to `parent`'s children: `parent` a group, a
/// radio (whose face is read from its children), or a group's new parent.
pub(crate) fn children_changed(
    arena: &NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    parent: u32,
) {
    if !matches!(metrics::column(), Column::Material | Column::Neutral) {
        return;
    }
    let group = |slot: u32| {
        matches!(
            arena.props(slot).str(PropId::AccessibilityRole),
            Some("radiogroup" | "tablist")
        )
    };
    let groups = std::iter::once(parent)
        .chain(arena.parent(parent))
        .find(|s| group(*s))
        .into_iter()
        .chain(arena.children(parent).iter().copied().filter(|c| group(*c)))
        .chain(navigation_lists(arena, parent))
        .chain(behind_keyed_children(arena, parent));
    for group in groups.collect::<Vec<_>>() {
        for slot in std::iter::once(group).chain(arena.children(group).iter().copied()) {
            if let Some(node) = arena.taffy(slot) {
                layout.restyle(arena, slot, node);
                layout.mark_dirty(node);
            }
        }
    }
}

/// The tablists behind a keyed child of `parent`: root-tab discovery
/// ([`navigation_lists`]) stops at a route key, so tabs moved into a keyed
/// container that is then attached would keep a root tab's layout.
fn behind_keyed_children(arena: &NodeArena, parent: u32) -> Vec<u32> {
    if metrics::column() != Column::Material {
        return Vec::new();
    }
    (arena.children(parent).iter())
        .filter(|c| arena.props(**c).get(PropId::NavigationKey).is_some())
        .flat_map(|c| arena.subtree(*c))
        .filter(|s| arena.props(*s).str(PropId::AccessibilityRole) == Some("tablist"))
        .collect()
}

/// Root-tab membership also reads panel ids and topology outside the group.
/// Refresh tablists in that navigation tree when those inputs move.
fn navigation_lists(arena: &NodeArena, slot: u32) -> Vec<u32> {
    if metrics::column() != Column::Material {
        return Vec::new();
    }
    let mut at = Some(slot);
    let mut root = slot;
    while let Some(s) = at {
        if arena.props(s).get(PropId::NavigationKey).is_some() {
            root = s;
        }
        at = arena.parent(s);
    }
    fn walk(arena: &NodeArena, slot: u32, lists: &mut Vec<u32>) {
        if arena.props(slot).str(PropId::AccessibilityRole) == Some("tablist") {
            lists.push(slot);
        }
        if arena.props(slot).get(PropId::NavigationKey).is_none() {
            for child in arena.children(slot) {
                walk(arena, *child, lists);
            }
        }
    }
    let mut lists = Vec::new();
    for child in arena.children(root) {
        walk(arena, *child, &mut lists);
    }
    lists
}
