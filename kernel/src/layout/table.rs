//! A table laid out as one grid (LLP 1153 D7.3). Its cells are placed where
//! HTML's table-forming algorithm puts them (HTML §4.9.12.1), so `colspan`
//! and `rowspan` land as the browser lands them, and the table's engine node
//! holds its captions, row groups, rows and cells as the grid's items: a
//! cell's column is the table's, whatever row holds it. A row or row group
//! keeps no engine children; its box spans its rows, and publication places
//! what it holds from the grid's origin, moved as the engine moved the row or
//! group from where the grid placed it ([`Tables::flat`], [`displacement`]).
//! An authored block table holds an anonymous table for each run of table
//! parts (CSS 2.2 §17.2.1). Flex and grid tables keep their direct items: each
//! blockified row group or row holds its own anonymous table. The columns
//! are `auto` tracks, the deviation `style/table.rs` declares.
//!
//! A change to a table's rows or cells marks it stale; its grid is made
//! again once, before the next layout ([`LayoutTree::refresh_tables`]).

use super::{order, LayoutTree};
use crate::arena::NodeArena;
use crate::generated::{Display, PropId};
use crate::id::{IdMap, IdSet};
use crate::props::PropValue;
use crate::style::table::{context, grid_spacing, table_of, Context, Part};
use taffy::prelude::{
    auto, length, line, span, GridPlacement, GridTemplateComponent, Line, NodeId,
};

/// Where a part stands in its table's grid, from 0: its first row and
/// column and how many of each it covers; `columns` 0 is every column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Area {
    pub row: u32,
    pub rows: u32,
    pub column: u32,
    pub columns: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    table: u32,
    area: Area,
    /// A cell's `colspan` and `rowspan` as it was placed by them.
    spans: (u32, u32),
    /// A row group or row: the table lays out what it holds.
    flat: bool,
    /// Which anonymous grid holds the part, when its owner is a block.
    grid: usize,
}

#[derive(Clone, Debug, Default)]
struct Formed {
    columns: u32,
    context: Option<Context>,
    direction: Option<taffy::style::Direction>,
    spacing: Option<(f32, f32)>,
    /// Its parts in grid order: captions, then each group and its rows, each
    /// row followed by its cells.
    parts: Vec<u32>,
    /// The table's engine padding, which its captions' margins follow; none
    /// for a table that is another box.
    padding: Option<taffy::geometry::Rect<taffy::style::LengthPercentage>>,
    /// The anonymous tables inside a block table or blockified table part.
    anon: Vec<Anonymous>,
    /// A block owner's shown children that are not its parts: each ends a
    /// run of parts, and so an anonymous table.
    boxes: Vec<u32>,
}

#[derive(Clone, Debug)]
struct Anonymous {
    columns: u32,
    parts: Vec<u32>,
    node: Option<NodeId>,
}

/// The highest line a table's grid places a part at (Taffy's
/// `grid_line_limit`, Exact's patch 70). A part's line is its row from 1, an
/// `i16` ([`place`]), so each row to `i16::MAX` has a track of its own, as in
/// Chrome. An authored grid keeps the engine's 10,000, past which its values
/// are refused (LLP 1001 §1).
const LINES: u16 = i16::MAX as u16;

/// The engine style of an anonymous table (CSS 2.2 §17.2.1): a grid of
/// `columns` `auto` tracks, shrink-to-fit, spaced as HTML's UA sheet
/// spaces a table's cells, or as the table's `border-spacing` does.
fn anonymous(
    columns: u32,
    direction: taffy::style::Direction,
    (across, down): (f32, f32),
) -> taffy::style::Style {
    let mut s = taffy::style::Style {
        display: taffy::style::Display::Grid,
        item_is_table: true,
        direction,
        padding: taffy::geometry::Rect {
            left: length(across),
            right: length(across),
            top: length(down),
            bottom: length(down),
        },
        gap: taffy::geometry::Size {
            width: length(across),
            height: length(down),
        },
        ..Default::default()
    };
    let track = GridTemplateComponent::Single(auto());
    s.rare.grid_template_columns = vec![track; columns as usize];
    s.rare.grid_line_limit = LINES;
    s
}

/// The tables an engine tree lays out as grids.
#[derive(Clone, Default)]
pub(crate) struct Tables {
    stale: IdSet<u32>,
    formed: IdMap<u32, Formed>,
    placed: IdMap<u32, Placed>,
}

impl Tables {
    /// Whether table formation owns `slot`'s engine children, marking its
    /// grid owner to be made again before layout.
    pub(super) fn holds(&mut self, arena: &NodeArena, slot: u32) -> bool {
        let table = if context(arena, slot).is_some() {
            slot
        } else {
            match Part::of(arena, slot) {
                Some(Part::Group | Part::Row) => match table_of(arena, slot) {
                    Some(table) => table,
                    None => return false,
                },
                _ => return false,
            }
        };
        self.stale.insert(table);
        true
    }

    /// Whether publication places `slot`'s children from the table's
    /// origin: `slot` is a row group or row of a table laid out as a grid.
    pub(super) fn flat(&self, slot: u32) -> bool {
        self.placed.get(&slot).is_some_and(|p| p.flat)
    }

    /// The tables among `slots` (a rebuilt or trial tree's), to be made.
    pub(super) fn find(&mut self, arena: &NodeArena, slots: &[u32]) {
        let tables = slots.iter().filter(|s| context(arena, **s).is_some());
        self.stale.extend(tables);
    }

    /// The first anonymous table holding `slot`'s parts, when it has one.
    pub(super) fn anon(&self, slot: u32) -> Option<NodeId> {
        self.formed.get(&slot)?.anon.first()?.node
    }

    /// The anonymous grid placing a direct child of `owner`.
    pub(super) fn grid_for(&self, owner: u32, child: u32) -> Option<NodeId> {
        let placed = self.placed.get(&child).filter(|p| p.table == owner)?;
        self.formed.get(&owner)?.anon.get(placed.grid)?.node
    }

    /// The table or blockified part whose anonymous table `node` is.
    pub(super) fn holding(&self, node: NodeId) -> Option<u32> {
        let mut tables = self.formed.iter();
        tables
            .find(|(_, f)| f.anon.iter().any(|g| g.node == Some(node)))
            .map(|(t, _)| *t)
    }

    /// A removed node's records, and the anonymous table it had.
    pub(super) fn forget(&mut self, slot: u32) -> Vec<NodeId> {
        self.stale.remove(&slot);
        self.placed.remove(&slot);
        let formed = self.formed.remove(&slot);
        self.placed.retain(|_, p| p.table != slot);
        formed
            .into_iter()
            .flat_map(|f| f.anon)
            .filter_map(|g| g.node)
            .collect()
    }

    /// `slot`'s engine style with its grid placement, or its table's
    /// columns. A change that moves the grid (a span, a part shown or
    /// hidden, a table's or a part's `display`, a box shown or hidden
    /// between a block's parts) marks the table to be made again.
    pub(super) fn style(&mut self, arena: &NodeArena, slot: u32, s: &mut taffy::style::Style) {
        if let Some(parent) = arena.parent(slot) {
            let owner = self.formed.get(&parent);
            if let Some(f) = owner.filter(|f| f.context == Some(Context::Block)) {
                if f.boxes.contains(&slot) != is_box(arena, parent, slot) {
                    self.stale.insert(parent);
                }
            }
        }
        let ctx = context(arena, slot);
        if ctx.is_some() || self.formed.contains_key(&slot) {
            match self.formed.get(&slot) {
                Some(formed) if formed.context == ctx => {
                    if ctx == Some(Context::Table) {
                        let track = GridTemplateComponent::Single(auto());
                        s.rare.grid_template_columns = vec![track; formed.columns as usize];
                        s.rare.grid_line_limit = LINES;
                        if formed.padding.as_ref() != Some(&s.padding) {
                            self.stale.insert(slot);
                        }
                    }
                    if (!formed.anon.is_empty() && formed.direction != Some(s.direction))
                        || formed.spacing != Some(grid_spacing(arena, slot))
                    {
                        self.stale.insert(slot);
                    }
                }
                _ => {
                    self.stale.insert(slot);
                }
            }
        }
        // Not a part placed in a grid (any more): the grid that placed it is
        // made again.
        let Some(part) = Part::of(arena, slot).filter(|_| ctx.is_none()) else {
            if let Some(p) = self.placed.get(&slot) {
                self.stale.insert(p.table);
            }
            return;
        };
        match (self.placed.get(&slot), shown_in(arena, slot)) {
            (Some(p), Some(table)) if p.table == table && p.spans == spans(arena, slot, part) => {
                place(p.area, s)
            }
            (None, None) => {}
            (p, table) => self
                .stale
                .extend(table.into_iter().chain(p.map(|p| p.table))),
        }
    }
}

/// What the engine moved `slot`, a row or row group, by from where its grid
/// placed it (Taffy's `grid_origin`): its `position: relative` offset (CSS 2.2
/// §9.4.3, as the engine resolves it: percentages, `calc()`, and `right` over
/// `left` right to left), its alignment in its area aside, or, for a popover
/// laid out before it has a block of its own (absolute in the top layer), how
/// far its insets put it from its area. A row's or row group's cells
/// are the grid's items beside it, not its children in the engine, so
/// publication moves them by this, and they follow their row's box. Anything
/// else (static, sticky, whose insets the host applies) moves nothing.
pub(super) fn displacement(arena: &NodeArena, tree: &LayoutTree, slot: u32) -> (f32, f32) {
    use crate::generated::PositionType;
    let moved = matches!(
        crate::style::top_layer::layout_position(arena, slot),
        PositionType::Relative | PositionType::Absolute
    );
    let Some(node) = arena.taffy(slot).filter(|_| moved) else {
        return (0.0, 0.0);
    };
    let Some(origin) = tree.grid_origin(node) else {
        return (0.0, 0.0);
    };
    let at = tree.layout(node).location;
    (at.x - origin.x, at.y - origin.y)
}

/// Whether `child` of a block owner is a shown box rather than one of its
/// parts.
fn is_box(arena: &NodeArena, owner: u32, child: u32) -> bool {
    arena.style(child).display != Display::None
        && !arena.blocks.holds(child)
        && table_of(arena, child) != Some(owner)
}

/// The table `slot` is placed in: `slot` and each part between it and the
/// table shown, as [`form`] places them.
fn shown_in(arena: &NodeArena, slot: u32) -> Option<u32> {
    let table = table_of(arena, slot)?;
    let mut at = slot;
    while at != table {
        if arena.style(at).display == Display::None {
            return None;
        }
        at = arena.parent(at)?;
    }
    Some(table)
}

/// A cell's `colspan` and `rowspan`, as HTML reads them (HTML §4.9.11): unset
/// or negative is 1, a `colspan` of 0 is 1, a `rowspan` of 0 is 0, the rest
/// of the cell's row group, and each at most HTML's limit.
fn spans(arena: &NodeArena, slot: u32, part: Part) -> (u32, u32) {
    if part != Part::Cell {
        return (1, 1);
    }
    let read = |id, least: i64, most: i64| {
        let n = match arena.props(slot).get(id) {
            Some(PropValue::Int(n)) => *n,
            Some(PropValue::Float(n)) if n.is_finite() => *n as i64,
            _ => 1,
        };
        if n < 0 {
            1
        } else {
            n.clamp(least, most) as u32
        }
    };
    (
        read(PropId::ColSpan, 1, 1000),
        read(PropId::RowSpan, 0, 65534),
    )
}

fn place(area: Area, s: &mut taffy::style::Style) {
    let at = |n: u32| line::<GridPlacement>(i16::try_from(n + 1).unwrap_or(i16::MAX));
    let cover = |n: u32| span::<GridPlacement>(u16::try_from(n.max(1)).unwrap_or(u16::MAX));
    s.rare.grid_row = Line {
        start: at(area.row),
        end: cover(area.rows),
    };
    s.rare.grid_column = if area.columns == 0 {
        Line {
            start: line(1),
            end: line(-1),
        }
    } else {
        Line {
            start: at(area.column),
            end: cover(area.columns),
        }
    };
    // A row group with no shown row has no area.
    if area.rows == 0 {
        s.display = taffy::style::Display::None;
    }
}

/// One placed part: its slot, its area, and a cell's spans as read.
struct Item {
    slot: u32,
    area: Area,
    spans: (u32, u32),
    grid: usize,
}

impl Item {
    fn new(slot: u32, area: Area, spans: (u32, u32)) -> Self {
        Self {
            slot,
            area,
            spans,
            grid: 0,
        }
    }
}

/// HTML's table-forming algorithm (HTML §4.9.12.1) over `table`'s shown
/// parts, as the browser lays it out: the captions first, each in a row of
/// its own; then the row groups and runs of rows in order, but the first
/// `thead` before them and the first `tfoot` after them (CSS 2.1 §17.2's
/// header and footer groups; a later one is a row group where it stands); a
/// cell's rows end with its row group's, where CSS's rendering ends them.
fn form(arena: &NodeArena, table: u32) -> (Formed, Vec<Item>) {
    let ctx = context(arena, table);
    if ctx.is_none() || ctx == Some(Context::Items) {
        return (
            Formed {
                context: ctx,
                ..Default::default()
            },
            Vec::new(),
        );
    }
    if ctx == Some(Context::Table) {
        return form_grid(arena, table, arena.children(table), ctx);
    }
    let mut formed = Formed {
        context: ctx,
        ..Default::default()
    };
    let mut placed = Vec::new();
    // Each run of misparented table parts gets its own anonymous table
    // (CSS 2.2 §17.2.1); ordinary boxes keep their place between runs.
    let children: Vec<u32> = arena
        .children(table)
        .iter()
        .copied()
        .filter(|s| arena.style(*s).display != Display::None && !arena.blocks.holds(*s))
        .collect();
    formed.boxes = (children.iter().copied())
        .filter(|s| is_box(arena, table, *s))
        .collect();
    for run in children.chunk_by(|a, b| {
        (table_of(arena, *a) == Some(table)) == (table_of(arena, *b) == Some(table))
    }) {
        if table_of(arena, run[0]) != Some(table) {
            continue;
        }
        let (grid, mut items) = form_grid(arena, table, run, ctx);
        if grid.parts.is_empty() {
            continue;
        }
        for item in &mut items {
            item.grid = formed.anon.len();
        }
        formed.parts.extend(&grid.parts);
        formed.anon.push(Anonymous {
            columns: grid.columns,
            parts: grid.parts,
            node: None,
        });
        placed.extend(items);
    }
    (formed, placed)
}

fn form_grid(
    arena: &NodeArena,
    table: u32,
    children: &[u32],
    ctx: Option<Context>,
) -> (Formed, Vec<Item>) {
    let of =
        |s: u32, part| Part::of(arena, s) == Some(part) && arena.style(s).display != Display::None;
    let mut out: Vec<Item> = Vec::new();
    let whole = |row, rows| Area {
        row,
        rows,
        column: 0,
        columns: 0,
    };
    let mut y = 0;
    for &c in children.iter().filter(|c| of(**c, Part::Caption)) {
        out.push(Item::new(c, whole(y, 1), (1, 1)));
        y += 1;
    }
    // Each group: its node, if it is one, and its rows; a run of rows the
    // table holds itself is a group of its own.
    let mut groups: Vec<(Option<u32>, Vec<u32>)> = Vec::new();
    let (mut head, mut foot) = (None, None);
    let mut run = Vec::new();
    for &c in children
        .iter()
        .filter(|_| Part::of(arena, table) == Some(Part::Table))
    {
        if of(c, Part::Row) {
            run.push(c);
            continue;
        }
        if !run.is_empty() {
            groups.push((None, std::mem::take(&mut run)));
        }
        if of(c, Part::Group) {
            let rows = arena.children(c).iter().copied();
            let group = (Some(c), rows.filter(|r| of(*r, Part::Row)).collect());
            match arena.props(c).str(PropId::SemanticTag) {
                Some("thead") if head.is_none() => head = Some(group),
                Some("tfoot") if foot.is_none() => foot = Some(group),
                _ => groups.push(group),
            }
        }
    }
    if !run.is_empty() {
        groups.push((None, run));
    }
    match Part::of(arena, table) {
        Some(Part::Group) => groups.push((
            None,
            children
                .iter()
                .copied()
                .filter(|s| of(*s, Part::Row))
                .collect(),
        )),
        Some(Part::Row) => groups.push((None, vec![table])),
        _ => {}
    }
    let groups = head.into_iter().chain(groups).chain(foot);
    let mut columns = 0;
    for (group, rows) in groups {
        if let Some(g) = group {
            out.push(Item::new(g, whole(y, rows.len() as u32), (1, 1)));
        }
        // Which of the group's slots a cell covers, by row from its first.
        let mut taken: Vec<Vec<bool>> = vec![Vec::new(); rows.len()];
        for (r, &row) in rows.iter().enumerate() {
            if row != table {
                out.push(Item::new(row, whole(y, 1), (1, 1)));
            }
            let mut x = 0u32;
            // A block row's run of cells, or a row's cells.
            let cells = if row == table {
                children
            } else {
                arena.children(row)
            };
            for &cell in cells.iter().filter(|c| of(**c, Part::Cell)) {
                while taken[r].get(x as usize).copied().unwrap_or(false) {
                    x += 1;
                }
                let (colspan, rowspan) = spans(arena, cell, Part::Cell);
                let left = (rows.len() - r) as u32;
                let covers = if rowspan == 0 {
                    left
                } else {
                    rowspan.min(left)
                };
                let end = (x + colspan) as usize;
                for slots in taken.iter_mut().skip(r).take(covers as usize) {
                    if slots.len() < end {
                        slots.resize(end, false);
                    }
                    slots[x as usize..end].fill(true);
                }
                let area = Area {
                    row: y,
                    rows: covers,
                    column: x,
                    columns: colspan,
                };
                out.push(Item::new(cell, area, (colspan, rowspan)));
                x += colspan;
                columns = columns.max(x);
            }
            y += 1;
        }
    }
    let parts = out.iter().map(|item| item.slot).collect();
    let formed = Formed {
        columns,
        parts,
        context: ctx,
        ..Formed::default()
    };
    (formed, out)
}

impl LayoutTree {
    /// Make every stale table's grid: its parts as its engine node's items,
    /// each placed, a row's and a row group's own children none. A part no
    /// longer in it, or a table that no longer lays out as one, gets its
    /// children and its style back. `node_for` is the tree's handle map.
    pub(super) fn refresh_tables(
        &mut self,
        arena: &NodeArena,
        node_for: &dyn Fn(u32) -> Option<NodeId>,
    ) {
        // A part restored from one table can stand in another, which its
        // restyle marks: that one is made in the next round.
        // A construction failure waits for the next refresh, retaining its
        // ownership and staleness rather than exhausting this round's bound.
        let mut failed = IdSet::default();
        for _ in 0..4 {
            if self.tables.stale.is_empty() {
                break;
            }
            let mut stale: Vec<u32> = self.tables.stale.drain().collect();
            stale.sort_unstable();
            for table in stale {
                if !self.make_table(arena, table, node_for) {
                    failed.insert(table);
                }
            }
            self.tables.stale.retain(|s| !failed.contains(s));
        }
        self.tables.stale.extend(failed);
    }

    fn make_table(
        &mut self,
        arena: &NodeArena,
        table: u32,
        node_for: &dyn Fn(u32) -> Option<NodeId>,
    ) -> bool {
        if !arena.is_live(table) {
            for anon in self.tables.forget(table) {
                self.remove(anon);
            }
            return true;
        }
        // Resolve and allocate before changing ownership: a missing handle
        // or engine refusal must leave the old grid available for retry.
        let Some(node) = node_for(table) else {
            return false;
        };
        let (mut formed, placed) = form(arena, table);
        let style = crate::style::taffy_style(arena, table);
        let laid = formed.context == Some(Context::Table);
        formed.padding = laid.then_some(style.padding);
        let spacing = grid_spacing(arena, table);
        formed.spacing = Some(spacing);
        let prior: Vec<NodeId> = self
            .tables
            .formed
            .get(&table)
            .into_iter()
            .flat_map(|f| &f.anon)
            .filter_map(|g| g.node)
            .collect();
        let mut allocated = Vec::new();
        for (i, grid) in formed.anon.iter_mut().enumerate() {
            grid.node = Some(if let Some(&node) = prior.get(i) {
                node
            } else {
                let style = self.share(anonymous(grid.columns, style.direction, spacing));
                match self.taffy.new_leaf(style) {
                    Ok(node) => {
                        self.made(node);
                        self.taffy.track_layout_input(node, false);
                        allocated.push(node);
                        node
                    }
                    Err(e) => {
                        for node in allocated {
                            self.remove(node);
                        }
                        self.fault.get_or_insert_with(|| format!("table: {e:?}"));
                        return false;
                    }
                }
            });
        }
        for grid in &formed.anon {
            self.write_style(
                grid.node.unwrap(),
                anonymous(grid.columns, style.direction, spacing),
            );
        }
        formed.direction = Some(style.direction);
        let before = self.tables.formed.remove(&table);
        let first = before.is_none();
        let before = before.unwrap_or_default();
        // Where each part stood: one that stays there keeps its engine style.
        let was: IdMap<u32, Placed> = (self.tables.placed.iter())
            .filter(|(_, p)| p.table == table)
            .map(|(s, p)| (*s, *p))
            .collect();
        self.tables.placed.retain(|_, p| p.table != table);
        for &Item {
            slot,
            area,
            spans,
            grid,
        } in &placed
        {
            let flat = matches!(Part::of(arena, slot), Some(Part::Group | Part::Row));
            let placed = Placed {
                table,
                area,
                spans,
                flat,
                grid,
            };
            self.tables.placed.insert(slot, placed);
        }
        // What left the grid holds its own children again, unless another
        // table placed it.
        let kept: IdSet<u32> = formed.parts.iter().copied().collect();
        let left = before.parts.iter().copied().filter(|s| {
            !kept.contains(s) && arena.is_live(*s) && !self.tables.placed.contains_key(s)
        });
        for slot in left.collect::<Vec<u32>>() {
            if let Some(node) = node_for(slot) {
                let before = self.alert_prefixes.get(&slot).copied();
                let ids = order::laid_out(arena, slot, before, node_for);
                self.set_children(node, &ids);
                self.restyle_with(arena, slot, node);
            }
        }
        for &Item { slot, .. } in &placed {
            if self.tables.flat(slot) {
                if let Some(node) = node_for(slot) {
                    self.set_children(node, &[]);
                }
            }
        }
        let items: Vec<NodeId> = formed.parts.iter().filter_map(|s| node_for(*s)).collect();
        if !formed.anon.is_empty() {
            for grid in &formed.anon {
                let items: Vec<NodeId> = grid.parts.iter().filter_map(|s| node_for(*s)).collect();
                self.set_children(grid.node.unwrap(), &items);
            }
            let mut ids = Vec::new();
            let mut last_grid = None;
            for &child in arena.children(table) {
                if arena.blocks.holds(child) || arena.style(child).display == Display::None {
                    continue;
                }
                if let Some(p) = self.tables.placed.get(&child).filter(|p| p.table == table) {
                    let grid = &formed.anon[p.grid];
                    if last_grid != grid.node {
                        ids.push(grid.node.unwrap());
                        last_grid = grid.node;
                    }
                } else if let Some(child) = node_for(child) {
                    ids.push(child);
                    last_grid = None;
                }
            }
            self.set_children(node, &ids);
        } else if laid {
            self.set_children(node, &items);
        } else if self.tables.flat(table) {
            self.set_children(node, &[]);
        } else {
            let ids = order::laid_out(arena, table, None, node_for);
            self.set_children(node, &ids);
        }
        for anon in before.anon.iter().filter_map(|g| g.node) {
            if !formed.anon.iter().any(|g| g.node == Some(anon)) {
                self.remove(anon);
            }
        }
        // New and reparented items can need grids even when the parent's
        // context stays the same. Cell spacing comes from that parent.
        let children = arena.children(table).iter().copied().filter(|s| {
            let ctx = context(arena, *s);
            let child = self.tables.formed.get(s);
            (ctx.is_some() || child.is_some())
                && (before.context != formed.context
                    || child.is_none_or(|f| {
                        f.context != ctx || f.spacing != Some(grid_spacing(arena, *s))
                    }))
        });
        self.tables.stale.extend(children);
        let restyle = first
            || before.columns != formed.columns
            || before.anon.is_empty() != formed.anon.is_empty()
            || before.context != formed.context;
        // Caption corrections depend on both padding and cell spacing.
        let padded = before.padding != formed.padding || before.spacing != formed.spacing;
        let parts = formed.parts.clone();
        if formed.context.is_some() {
            self.tables.formed.insert(table, formed);
        }
        if restyle {
            self.restyle_with(arena, table, node);
        }
        for slot in parts {
            let moved = was.get(&slot) != self.tables.placed.get(&slot);
            let caption = padded && Part::of(arena, slot) == Some(Part::Caption);
            if let Some(node) = node_for(slot).filter(|_| moved || caption) {
                self.restyle_with(arena, slot, node);
            }
        }
        true
    }

    /// Whether table formation has a record of `slot`: a grid it owns or
    /// one it is placed in.
    pub(crate) fn in_table(&self, slot: u32) -> bool {
        self.tables.formed.contains_key(&slot) || self.tables.placed.contains_key(&slot)
    }

    /// `slot`'s engine style as `restyle` derives it.
    pub(super) fn restyle_with(&mut self, arena: &NodeArena, slot: u32, node: NodeId) {
        let mut style = crate::style::taffy_style(arena, slot);
        self.tables.style(arena, slot, &mut style);
        self.set_style(node, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kernel, NodeType, Offer, Op, StyleId, StyleProps, StyleValue};

    #[test]
    fn a_missing_table_node_preserves_ownership_and_retries() {
        let mut k = Kernel::with_monospace();
        let mut patch = StyleProps::default();
        patch
            .set_dynamic(StyleId::Display, &StyleValue::Text("block".into()))
            .unwrap();
        k.apply(
            0,
            1,
            &[
                Op::CreateView {
                    id: 1,
                    node_type: NodeType::View,
                },
                Op::CreateView {
                    id: 2,
                    node_type: NodeType::View,
                },
                Op::CreateView {
                    id: 3,
                    node_type: NodeType::View,
                },
                Op::SetProp {
                    id: 1,
                    prop: PropId::SemanticTag,
                    value: "table".into(),
                },
                Op::SetProp {
                    id: 2,
                    prop: PropId::SemanticTag,
                    value: "tr".into(),
                },
                Op::SetProp {
                    id: 3,
                    prop: PropId::SemanticTag,
                    value: "td".into(),
                },
                Op::SetStyle {
                    id: 1,
                    patch: Box::new(patch),
                },
                Op::SetChildren {
                    id: 1,
                    children: vec![2],
                },
                Op::SetChildren {
                    id: 2,
                    children: vec![3],
                },
                Op::AttachRoot { id: 1 },
            ],
        )
        .unwrap();
        k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
        let arena = k.arena().clone();
        let table = arena.resolve(k.node(1).unwrap().key).unwrap();
        let row = arena.resolve(k.node(2).unwrap().key).unwrap();
        let mut tree = k.tree().clone();
        let anon = tree.tables.anon(table).unwrap();
        let placed = tree.tables.placed.clone();
        tree.tables.stale.insert(table);
        tree.refresh_tables(&arena, &|s| (s != table).then(|| arena.taffy(s)).flatten());
        assert_eq!(tree.tables.anon(table), Some(anon));
        assert_eq!(tree.tables.placed, placed);
        assert!(tree.tables.stale.contains(&table));
        tree.refresh_tables(&arena, &|s| arena.taffy(s));
        assert_eq!(tree.tables.anon(table), Some(anon));
        assert!(tree.tables.flat(row));
        assert!(tree.tables.stale.is_empty());
        assert_eq!(
            tree.taffy.total_node_count(),
            k.tree().taffy.total_node_count()
        );
    }
}
