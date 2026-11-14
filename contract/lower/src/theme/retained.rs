//! The design server's build, stopped at the design's retention (LLP 1121
//! stage 12 B1's follow-up, theme2's 11:32 ruling).

use super::{
    rows,
    scan::{Read, Said},
};
use contract_syntax::{Component, File};
use std::cell::RefCell;
use std::sync::Arc;

/// A validated expansion's token uses and grouped row map. Only the
/// compiler driver may offer this again, after validating its loaded
/// program, strings, types and expansion (`contract::retained`).
#[derive(Clone)]
pub struct DesignScan {
    declared: Arc<rows::Declared>,
    views: std::collections::BTreeMap<String, crate::DeclaredView>,
    read: Read,
    rows: Vec<rows::Row>,
}

#[derive(Default)]
struct Scanning {
    keep: bool,
    scan: Option<DesignScan>,
}

thread_local! {
    /// Lowering stops once the design's retention and synthetic tokens are
    /// known ([`retention_only`]).
    static RETENTION_ONLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FIRST_REFUSAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BEFORE_VIEW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SCANNING: RefCell<Scanning> = RefCell::new(Scanning::default());
}

/// Scope reuse of a validated expansion's design scan to one build.
pub fn with_design_scan<R>(run: impl FnOnce() -> R) -> R {
    struct Scope(Option<Scanning>);
    impl Drop for Scope {
        fn drop(&mut self) {
            SCANNING.with(|s| *s.borrow_mut() = self.0.take().unwrap_or_default());
        }
    }
    let outer = SCANNING.with(|s| {
        s.replace(Scanning {
            keep: true,
            scan: None,
        })
    });
    let _scope = Scope(Some(outer));
    run()
}

/// Offer the scan only after validating the program it was computed from.
pub fn offer_design_scan(scan: Option<DesignScan>) {
    SCANNING.with(|s| s.borrow_mut().scan = scan);
}

/// Take this build's design scan for reuse with its checked expansion.
pub fn take_design_scan() -> Option<DesignScan> {
    SCANNING.with(|s| s.borrow_mut().scan.take())
}

pub(super) fn scan(
    file: &File,
    root: &Component,
    design: &exact_tamber::Design,
    declared: &Arc<rows::Declared>,
    typing: &rows::Typing,
) -> Said {
    let keep = SCANNING.with(|s| s.borrow().keep);
    if !keep {
        let mut read = Read::of(file, root, declared, typing);
        rows::note(std::mem::take(&mut read.rows));
        rows::note_left(read.left.clone());
        return read.bind(design);
    }
    // The expansion is scanned independently of the design. Fonts are read anew:
    // a changed font file can change a row's font checks without moving
    // the program whose types the driver validated.
    let views = crate::native_typed::declarations();
    let offered = take_design_scan().filter(|s| s.declared == *declared && s.views == views);
    let scan = offered.unwrap_or_else(|| {
        let mut read = Read::of(file, root, declared, typing);
        let rows = rows::grouped(std::mem::take(&mut read.rows));
        DesignScan {
            declared: declared.clone(),
            views,
            read,
            rows,
        }
    });
    if !stops_at_refusal() {
        rows::note_grouped(scan.rows.clone());
        rows::note_left(scan.read.left.clone());
    }
    let said = scan.read.bind(design);
    offer_design_scan(Some(scan));
    said
}

/// Lower, on this thread, only as far as the design's retention and
/// synthetic tokens (`retention`, `synthetic_candidates`), which are
/// fixed before the design is folded: the design server's, which reads
/// nothing else of the build (stage 12 B1's follow-up). The build then
/// stops with no refusal of its own; every pass before lowering runs as
/// it always does. The setting it replaces.
pub fn retention_only(on: bool) -> bool {
    RETENTION_ONLY.with(|r| r.replace(on))
}

/// Whether lowering stops at the design's retention ([`retention_only`]).
pub(crate) fn stops_at_retention() -> bool {
    RETENTION_ONLY.with(|r| r.get())
}

/// Stop lowering after its first refusal. The setting it replaces.
pub fn first_refusal(on: bool) -> bool {
    FIRST_REFUSAL.with(|r| r.replace(on))
}

pub(crate) fn stops_at_refusal() -> bool {
    FIRST_REFUSAL.with(|r| r.get())
}

/// Stop lowering before the view's first element: what the design, the
/// declarations and the bodies refuse, which precede any element's
/// refusal. The setting it replaces.
pub fn before_view(on: bool) -> bool {
    BEFORE_VIEW.with(|r| r.replace(on))
}

pub(crate) fn stops_before_view() -> bool {
    BEFORE_VIEW.with(|r| r.get())
}
