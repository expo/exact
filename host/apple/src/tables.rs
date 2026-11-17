//! @ref LLP 1153 D7.3 — the kernel's table formation for the presenter's
//! table accessibility: UIKit's data table and AppKit's `AXTable` say how
//! many rows and columns a table has, where each cell stands, and visit the
//! rows in the order the table draws them (`Batch::table`).
use super::*;
use std::fmt::Write as _;

impl<D: DataSource> Host<D> {
    /// Each `table`'s size and drawn order and each `tr`, `td` and `th`'s
    /// place, when they changed. A part another box placed (a row outside
    /// any `table`, in an anonymous one) is sent too: the presenter keeps a
    /// place only under the table that made it.
    pub(super) fn emit_tables(&mut self, batch: &mut Batch) {
        let kernel = self.runner.kernel();
        let views = kernel.table_views();
        if views.is_empty() && self.tables.is_empty() {
            return;
        }
        let mut now = IdMap::default();
        for id in views {
            let Some(node) = kernel.node(id) else {
                continue;
            };
            let mut record = String::new();
            match node.props.str(PropId::SemanticTag) {
                Some("table") => {
                    let Some((rows, columns)) = kernel.table_size(id) else {
                        continue;
                    };
                    let _ = write!(record, ",\"size\":[{rows},{columns}]");
                    if let Some(order) = kernel.table_order(id) {
                        let children = node.children();
                        let drawn: Vec<String> = (order.iter())
                            .filter_map(|i| children.get(*i).map(u32::to_string))
                            .collect();
                        let _ = write!(record, ",\"order\":[{}]", drawn.join(","));
                    }
                }
                Some("tr" | "td" | "th") => {
                    let Some(p) = kernel.table_place(id) else {
                        continue;
                    };
                    let _ = write!(
                        record,
                        ",\"table\":{},\"place\":[{},{},{},{}]",
                        p.table, p.row, p.rows, p.column, p.columns
                    );
                }
                _ => continue,
            }
            if self.tables.get(&id) != Some(&record) {
                batch.table(id, &record);
            }
            now.insert(id, record);
        }
        for id in self.tables.keys() {
            if !now.contains_key(id) && kernel.node(*id).is_some() {
                batch.table(*id, "");
            }
        }
        self.tables = now;
    }
}
