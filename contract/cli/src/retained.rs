//! The design server's retention build (LLP 1121 stage 12 B1's follow-up,
//! theme2's 11:32 ruling): the pipeline as any build runs it, stopped once
//! the design's retention and synthetic tokens are known
//! (`contract_lower::retention_only`), and, on a build whose Contract the
//! design cannot have moved, the types and expansion of the last one kept
//! ([`Kept`]) in place of checking them again.

use contract_syntax::{Expanded, File};
use contract_types::{strings::Strings, Types};
use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

/// What a build's types and expansion are computed from, and them: the
/// loaded program with what the loader reads of the designs cleared, its
/// strings, whether it was checked mapped.
#[derive(Clone)]
pub struct Kept {
    file: File,
    strings: Option<Arc<Strings>>,
    mapped: bool,
    types: Types,
    expanded: Expanded,
    scan: Option<contract_lower::DesignScan>,
}

/// What [`retention_build`] found.
pub struct Retained {
    /// The plan's retention (D7): none when the build stopped before its
    /// design.
    pub retention: Option<exact_tamber::Retention>,
    /// The app's synthetic tokens (D3), each its expression and whether a
    /// colour row writes it.
    pub candidates: Vec<(String, bool)>,
    /// This build's types and expansion, for the next.
    pub kept: Option<Kept>,
    /// Whether `kept` was reused, not checked again.
    pub reused: bool,
    /// Under `EXACT_TAMBER_RETENTION_VERIFY` (a test's): the reused types
    /// or expansion were not what checking again gives, and the fresh ones
    /// were used.
    pub stale: bool,
    /// The style rows whose values are literal, which the design server's
    /// write check judges again (`contract_lower::rows`).
    pub rows: Vec<contract_lower::rows::Row>,
    /// Each source's path, by source id: where a row is.
    pub paths: Vec<std::path::PathBuf>,
    /// What those rows leave to the build.
    pub left: contract_lower::rows::Left,
}

/// How far [`check_path_reusing`] lowers a proposed design.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stop {
    /// Before the view's first element: the refusals that precede every
    /// element's, the surfaces' arguments not checked.
    View,
    /// The whole build, to its first refusal.
    Refusal,
}

#[derive(Default)]
struct Program {
    /// Offered for reuse to this thread's next check.
    offered: Option<Kept>,
    /// Keep the next check's.
    keep: bool,
    kept: Option<Kept>,
    reused: bool,
    stale: bool,
    paths: Vec<std::path::PathBuf>,
}

thread_local! {
    static PROGRAM: RefCell<Program> = RefCell::new(Program::default());
}

/// Restore the compiler's thread state even when a caller catches a panic.
struct Scope(Option<Program>, bool, bool, bool);

impl Drop for Scope {
    fn drop(&mut self) {
        contract_lower::retention_only(self.1);
        contract_lower::first_refusal(self.2);
        contract_lower::before_view(self.3);
        let outer = self.0.take().unwrap_or_default();
        PROGRAM.with(|p| *p.borrow_mut() = outer);
    }
}

/// Run `run` on this thread's compiler with `offered` to reuse: stopped at
/// the design's retention (`None`) or as `stop` says.
fn scoped<R>(offered: Option<Kept>, stop: Option<Stop>, run: impl FnOnce() -> R) -> (R, Program) {
    let outer = PROGRAM.with(|p| {
        std::mem::replace(
            &mut *p.borrow_mut(),
            Program {
                offered,
                keep: true,
                ..Program::default()
            },
        )
    });
    let scope = Scope(
        Some(outer),
        contract_lower::retention_only(stop.is_none()),
        contract_lower::first_refusal(stop.is_some()),
        contract_lower::before_view(stop == Some(Stop::View)),
    );
    let result = contract_lower::with_design_scan(run);
    let program = PROGRAM.with(|p| std::mem::take(&mut *p.borrow_mut()));
    drop(scope);
    (result, program)
}

/// `file` as types and expansion read it: the designs, which only lowering
/// reads, cleared (what the loader derives from them into the views, a
/// motion token's value, stays and is compared).
fn cleared(file: &File) -> File {
    File {
        design: None,
        package_designs: Vec::new(),
        ..file.clone()
    }
}

/// Build the app at `path` as far as its design's retention, reusing
/// `offered`'s types and expansion where the loaded program, its strings
/// and how it is checked are what they were computed from; else checking
/// them as any build does. Pure but for the files read and this thread's
/// lowering state.
pub fn retention_build(path: &Path, offered: Option<Kept>) -> Retained {
    let _ = contract_lower::synthetic_candidates();
    let _ = contract_lower::retention();
    let _ = contract_lower::rows::rows();
    let _ = contract_lower::rows::left();
    let (_, program) = scoped(offered, None, || crate::compile_path_all(path, false));
    Retained {
        retention: contract_lower::retention(),
        candidates: contract_lower::synthetic_candidates(),
        kept: program.kept,
        reused: program.reused,
        stale: program.stale,
        rows: contract_lower::rows::rows(),
        paths: program.paths,
        left: contract_lower::rows::left(),
    }
}

/// What [`check_path_reusing`] found.
pub struct Check {
    /// The build's first refusal, as far as it lowered.
    pub first: Option<crate::CompileError>,
    /// Whether the sources loaded to the program `kept` held (a design's
    /// motion tokens are written into it), whose work was reused.
    pub reused: bool,
}

/// The first refusal of the build of `path`, lowered as far as `stop`
/// says, reusing `kept`'s types, expansion and design scan where the
/// sources load to the program they were computed from (a design that
/// writes other motion values into it is checked again). `kept` is left
/// holding that program's work.
pub fn check_path_reusing(path: &Path, kept: &mut Option<Kept>, stop: Stop) -> Check {
    let build = || {
        let src = crate::read_source(path).map_err(|e| vec![e])?;
        let surfaces = stop == Stop::Refusal;
        crate::compile_path_checked(path, &src, false, surfaces, contract_lower::Profile::Web)
    };
    let (result, program) = scoped(kept.take(), Some(stop), build);
    *kept = match program.reused {
        true => program.kept,
        false => program.offered,
    };
    Check {
        first: result.err().and_then(|all| all.into_iter().next()),
        reused: program.reused,
    }
}

/// The offered types and expansion, when `file`, `strings` and `mapped`
/// are what they were computed from; none otherwise, or when nothing is
/// offered (any build but the design server's).
pub(crate) fn reuse(
    file: &File,
    strings: &Option<Arc<Strings>>,
    mapped: bool,
) -> Option<(Types, Expanded)> {
    let offered = PROGRAM.with(|p| p.borrow_mut().offered.take())?;
    // A design that doesn't parse is a doubt, whatever the loader made of
    // it: checked again (condition (c)).
    let parses = file
        .design
        .iter()
        .chain(&file.package_designs)
        .all(|d| exact_tamber::parse(&d.text).is_ok());
    let same = parses
        && offered.mapped == mapped
        && offered.strings == *strings
        && offered.file == cleared(file);
    if !same {
        // Declined, it stays offered: a write check keeps it.
        PROGRAM.with(|p| p.borrow_mut().offered = Some(offered));
        return None;
    }
    PROGRAM.with(|p| p.borrow_mut().reused = true);
    contract_lower::offer_design_scan(offered.scan);
    Some((offered.types, offered.expanded))
}

/// Keep what `file` checked to, when the design server asked.
pub(crate) fn keep(
    file: &File,
    strings: &Option<Arc<Strings>>,
    mapped: bool,
    checked: contract_types::Checked<'_>,
) {
    PROGRAM.with(|p| {
        let mut p = p.borrow_mut();
        if p.keep {
            p.kept = Some(Kept {
                file: cleared(file),
                strings: strings.clone(),
                mapped,
                types: checked.types,
                expanded: checked.expanded,
                scan: contract_lower::take_design_scan(),
            });
        }
    })
}

/// Whether reused types and expansion are to be checked against fresh
/// ones (`EXACT_TAMBER_RETENTION_VERIFY`, theme2's condition (b)): a
/// test's, so that a pass that comes to read the design fails a test
/// instead of going stale.
pub(crate) fn verifying() -> bool {
    std::env::var_os("EXACT_TAMBER_RETENTION_VERIFY").is_some()
}

/// Whether the `reused` types and expansion are the `fresh` ones; a
/// mismatch is said ([`Retained::stale`]).
pub(crate) fn verify(fresh: (&Types, &Expanded), reused: (&Types, &Expanded)) -> bool {
    let same = format!("{:?}", fresh.0) == format!("{:?}", reused.0) && fresh.1 == reused.1;
    if !same {
        PROGRAM.with(|p| p.borrow_mut().stale = true);
        contract_lower::offer_design_scan(None);
    }
    same
}

/// Checking again refused what reuse would have taken (condition (b)): said
/// as [`Retained::stale`].
pub(crate) fn refused_fresh() {
    PROGRAM.with(|p| p.borrow_mut().stale = true);
}

/// Keep each source's path, when the design server asked: where its rows
/// are.
pub(crate) fn keep_paths(paths: &[std::path::PathBuf]) {
    PROGRAM.with(|p| {
        let mut p = p.borrow_mut();
        if p.keep {
            p.paths = paths.to_vec();
        }
    })
}
