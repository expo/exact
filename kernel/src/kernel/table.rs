//! Where a table's rows and cells stand (LLP 1153 D7.3), for the hosts'
//! table accessibility: Android's `CollectionInfo` and `CollectionItemInfo`,
//! AccessKit's row and column indices and spans, UIKit's data table.

use super::Kernel;
use crate::id::ViewId;
use crate::layout::LayoutMirror;

/// Where a table's row or cell stands, counted in the order the table is
/// drawn (CSS 2.2 §17.2): the first `thead` first and the first `tfoot`
/// last, any other row group where it is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TablePlace {
    /// The table that placed it: a `table`, or the box holding the
    /// anonymous table a misparented row or cell is in (CSS 2.2 §17.2.1).
    pub table: ViewId,
    /// Its first row, from 0, among the table's rows: a caption is not one.
    pub row: u32,
    /// How many rows it covers: a row 1, a cell its `rowspan`, ended with
    /// its row group.
    pub rows: u32,
    /// Its first column, from 0.
    pub column: u32,
    /// How many columns it covers: a row every one, a cell its `colspan`.
    pub columns: u32,
}

impl Kernel {
    /// Where `view`, a `tr`, `td` or `th`, stands in its table, as of the
    /// last layout; `None` for anything else, a part no table places, and
    /// a kernel that lays nothing out.
    pub fn table_place(&self, view: ViewId) -> Option<TablePlace> {
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref)?;
        let (table, area) = tree.table_place(&self.arena, self.arena.slot_of(view)?)?;
        let columns = match area.columns {
            0 => tree.table_size(table).map_or(0, |(_, columns)| columns),
            n => n,
        };
        Some(TablePlace {
            table: self.arena.local_id(table),
            row: area.row,
            rows: area.rows,
            column: area.column,
            columns,
        })
    }

    /// The order `view`'s table draws its children in, as of the last
    /// layout (CSS 2.2 §17.2), as each child's index among them as written:
    /// the captions, the first `thead`, the other row groups and rows as
    /// written, then the first `tfoot`. A child the table does not place,
    /// such as a box between a block table's anonymous tables, keeps its
    /// index. `None` when `view` draws its children as written.
    pub fn table_order(&self, view: ViewId) -> Option<Vec<usize>> {
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref)?;
        tree.table_order(&self.arena, self.arena.slot_of(view)?)
    }

    /// The views table formation laid out at the last layout, in no order:
    /// each table and each caption, row group, row and cell it placed. The
    /// three answers above say nothing of any other view.
    pub fn table_views(&self) -> Vec<ViewId> {
        let Some(tree) = self.layout.as_deref().and_then(LayoutMirror::tree_ref) else {
            return Vec::new();
        };
        (tree.table_slots())
            .map(|slot| self.arena.local_id(slot))
            .collect()
    }

    /// How many rows and columns `view`, a table, has as of the last
    /// layout, its captions aside; `None` for a node no table formation
    /// lays out as one (a `table` whose author made it flex or grid).
    pub fn table_size(&self, view: ViewId) -> Option<(u32, u32)> {
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref)?;
        tree.table_size(self.arena.slot_of(view)?)
    }
}
