//! The list engine's vocabulary (LLP 1047.000 §9): what hosts, the core's
//! fields and the core's fronts name, with or without the engine linked.
mod feedback;
mod into_view;
mod reorder;

use exact_plan::Value;
pub use feedback::*;
pub use into_view::{Align, IntoView, IntoViewStatus};
pub use reorder::*;
use std::rc::Rc;

pub(crate) const BOOTSTRAP_ROWS: usize = 16;
pub(crate) const ESTIMATED_HEIGHT: f64 = 32.0;

/// Rows mounted out of their port and rows that showed, for the commit that
/// carries the batch ([`exact_kernel::Kernel::await_view`],
/// [`exact_kernel::CommitReceipt::revealed`]).
#[derive(Debug, Default, Clone)]
pub struct RowsShown {
    /// Wrappers of rows mounted (or bound again) out of the port.
    pub awaiting: Vec<exact_kernel::ViewId>,
    /// Wrappers of rows that waited and now show.
    pub revealed: Vec<exact_kernel::ViewId>,
}

impl RowsShown {
    /// Add another update's rows.
    pub fn extend(&mut self, other: RowsShown) {
        self.awaiting.extend(other.awaiting);
        self.revealed.extend(other.revealed);
    }
}

/// The table rows a sectioned collection exposes to a native data source.
#[derive(Debug, PartialEq, Eq)]
pub struct SectionTable {
    /// The logical row kinds, including sections outside the window.
    pub runs: Rc<[SectionRun]>,
    /// Each mounted section's item index and the views for its table rows.
    pub mounted: Vec<(usize, Vec<exact_kernel::ViewId>)>,
}

/// Consecutive collection items with the same section rows. A header is
/// `true`, a body row `false`; a footer has no row (LLP 1115.002 §4).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SectionRun {
    /// Exclusive item index at the end of the run.
    pub end: usize,
    /// Row kinds in each of those sections, in authored order.
    pub headers: Vec<bool>,
}

/// The host's paragraph and `user-select: auto` rules for logical copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListTextPolicy {
    /// DOM paragraphs, with the host stylesheet's unselectable control labels.
    Web,
    /// AppKit labels/chrome are unselectable; native titles have no paragraph.
    MacOS,
}

/// A text position that survives retirement of a list row's native views.
#[derive(Debug, Clone, Copy)]
pub struct ListTextPosition<'a> {
    /// The opaque row-wrapper key; ignored for fixed positions.
    pub key: &'a str,
    /// Fixed lead (`Some(false)`) or trail (`Some(true)`); `None` addresses a row.
    pub fixed: Option<bool>,
    /// Zero-based paragraph within the row or fixed region.
    pub paragraph: usize,
    /// UTF-16 offset within that paragraph, as on the DOM and Apple text APIs.
    pub offset: usize,
}

/// The most keys a list keeps, `initial-item-count`'s bound: kept rows add
/// at most this many to any window. The set is the author's and does not
/// grow as the reader scrolls (§6.2's memory requirement).
pub(crate) const MOST: usize = 64;

/// The row identities `value` names: a list of at most [`MOST`] keys, each
/// a string, number or bool or an option of one (`none` names no row). The
/// first row of a repeated key is the one kept. `None` for any other value.
pub(crate) fn idents(value: &Value) -> Option<Vec<Rc<str>>> {
    let Value::List(items) = value else {
        return None;
    };
    if items.len() > MOST {
        return None;
    }
    let mut out: Vec<Rc<str>> = Vec::new();
    for item in items.iter() {
        let key = match item {
            Value::Option(None) => continue,
            Value::Option(Some(key)) => key,
            key => key,
        };
        let text: Rc<str> = crate::instance::key_text(key)?.into();
        if !out.contains(&text) {
            out.push(text);
        }
    }
    Some(out)
}

/// 1.0's bits: the process's lead scale (a host may boot on one thread and
/// run on another).
pub(crate) static LEAD_SCALE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0x3FF0_0000_0000_0000);

/// How much of a window's lead past its viewport collections realize: 1,
/// the default, all of it; a host drawing its first frame may ask for 0 (the
/// rows that show), then 1 once that frame is out, so the rows past the
/// viewport mount after the first frame instead of before it.
pub fn set_lead_scale(scale: f64) {
    LEAD_SCALE.store(
        scale.clamp(0.0, 1.0).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// The provisional extent (px along the axis) a list with no port yet
/// realizes rows for: by default sixteen 32 px rows' worth; a host that knows
/// its viewport sets it ([`set_bootstrap_extent`]).
pub(crate) static BOOTSTRAP_EXTENT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new((BOOTSTRAP_ROWS as f64 * ESTIMATED_HEIGHT).to_bits());

/// How far a new list's first rows reach before its port is reported: a
/// host passes its viewport's extent, so the rows that can show are built
/// with the list instead of in a second commit after the first layout (at
/// most sixteen rows; a list's `initial-item-count` still wins).
pub fn set_bootstrap_extent(px: f64) {
    if px.is_finite() && px > 0.0 {
        BOOTSTRAP_EXTENT.store(px.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}
