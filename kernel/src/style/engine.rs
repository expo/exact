//! Values lowered into the layout engine.
use super::StyleProps;
use crate::generated::{GridAutoFlow, Overflow};

/// The live slot's engine style in the containing block its host supplied, or for a root the window's
/// aspect-fit guide fills (LLP 1123 D16), that guide.
pub fn taffy_style(arena: &crate::arena::NodeArena, slot: u32) -> taffy::style::Style {
    let block = arena
        .blocks
        .get(slot)
        .or_else(|| arena.blocks.root_fits().block(arena.key(slot)));
    super::style_in_block(arena, slot, block)
}

pub(super) fn rare(style: &StyleProps, env: &crate::Env) -> taffy::style::Rare {
    taffy::style::RareStyle {
        grid_template_columns: style.rare.grid_template_columns.taffy_components(env),
        grid_template_column_names: style.rare.grid_template_columns.line_names(),
        grid_template_rows: style.rare.grid_template_rows.taffy_components(env),
        grid_template_row_names: style.rare.grid_template_rows.line_names(),
        grid_column: style.rare.grid_column.taffy(),
        grid_row: style.rare.grid_row.taffy(),
        ..taffy::style::RareStyle::DEFAULT
    }
    .into()
}

pub(super) fn overflow(v: Overflow) -> taffy::style::Overflow {
    match v {
        Overflow::Visible => taffy::style::Overflow::Visible,
        Overflow::Hidden => taffy::style::Overflow::Hidden,
        Overflow::Clip => taffy::style::Overflow::Clip,
        Overflow::Scroll | Overflow::Auto => taffy::style::Overflow::Scroll,
    }
}

pub(super) fn grid_auto_flow(v: GridAutoFlow) -> taffy::style::GridAutoFlow {
    match v {
        GridAutoFlow::Row => taffy::style::GridAutoFlow::Row,
        GridAutoFlow::Column => taffy::style::GridAutoFlow::Column,
        GridAutoFlow::Dense | GridAutoFlow::RowDense => taffy::style::GridAutoFlow::RowDense,
        GridAutoFlow::ColumnDense => taffy::style::GridAutoFlow::ColumnDense,
    }
}
