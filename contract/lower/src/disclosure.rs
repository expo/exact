//! @ref LLP 1084.000 D1, D5 — a row's disclosure: the literal forward
//! chevron a list's row owns. It is found here once, on the expanded view, and
//! the element's lowering gives it the mark (`rowDisclosure`, `semantic` or
//! `always`), which the hosts are to read in place of the glyph's name to show
//! or hide it by their own platform's convention for the list it is in.
//!
//! A row is a child of a `list`, or a row in a `section`'s body, drawn as a
//! card or not; a section's `header` and `footer` are not rows. The walk goes
//! from each row's root, which may itself be a `link` or a `button`, through
//! its boxes, its `if`, `match` and `for` branches and a control that holds
//! every text the row has (its push, a `link` beside a row's star button), and
//! stops at a control that holds none or not all of them (a Next button, an
//! expander, a labelled More beside the row's title), at an image with a press
//! of its own, at a presentation, and at a nested list, whose rows are walked
//! on their own. The row's texts are its text elements (`text`, `p`, `time`,
//! `strong`, `em`) outside its presentations and its nested lists. Every
//! eligible image a row holds is a mark, with its own mode. A chevron spelled
//! by a template, or by a name an icon the app declares resolves to, is artwork.

use crate::error::LowerError;
use crate::IMPLIED;
use contract_checking::{Checking, Visit};
use contract_syntax::{Attr, Expr, Span};
use contract_types::Checked;
use std::cell::RefCell;
use std::collections::HashMap;

/// The prop a marked image carries.
const MARK: &str = "rowDisclosure";
/// The attribute an author writes on a mark.
const AUTHORED: &str = "disclosure";

thread_local! {
    /// The view's marks, by their element's span and instance, and each one's mode.
    static MARKS: RefCell<HashMap<(Span, u32), &'static str>> = RefCell::new(HashMap::new());
}

/// Forget the last view's marks, before lowering can stop short of finding this view's.
pub(crate) fn forget() {
    MARKS.with_borrow_mut(HashMap::clear);
}

/// Where the walk stands.
#[derive(Clone, PartialEq)]
enum Place {
    /// Outside every row.
    Outside,
    /// A list's children: its rows, or its sections.
    Rows,
    /// A section's children: its rows, its header and its footer.
    SectionRows,
    /// Inside a row, which holds this many texts.
    Row(usize),
    /// Inside a swipe row, until the content its `swipeContent` names.
    Swipe(Expr),
}

/// What the walk saw of one identity, an element's span and instance, which an
/// expansion can place more than once (a slot's one fill, placed twice).
#[derive(Default)]
struct Seen {
    tag: String,
    /// It stood as a row's chevron.
    marked: bool,
    /// It stood as a chevron that is no row's.
    artwork: bool,
    /// Its author's `disclosure`.
    authored: Option<Attr>,
    /// An author's own `rowDisclosure`.
    written_mark: Option<Span>,
}

/// Find each row's chevrons in `checked`'s expanded view, and refuse a
/// `disclosure` that is not on one, or whose value is not the literal string
/// `"always"`. Each identity is settled first and refused at most once: one an
/// expansion shows both as a row's chevron and as artwork is artwork.
pub(crate) fn find_row_disclosures(checked: &Checked<'_>) -> Vec<LowerError> {
    let checking = Checking::new(checked);
    let view = checking.expanded();
    let mut seen: HashMap<(Span, u32), Seen> = HashMap::new();
    let walk = view.walk_with(Place::Outside, |v, place| {
        let Some((tag, positional, attrs)) = v.element() else {
            return place.clone();
        };
        // Take no presentation's own image for a row's chevron: it is the presentation's
        let chevron = literal_chevron(tag, positional, attrs) && !presentation(tag, attrs);
        let (marked, below) = match place {
            // A list's children are its rows wherever it stands, a swipe row's content included
            _ if tag == "list" && !presentation(tag, attrs) => (false, Place::Rows),
            Place::Outside => (false, Place::Outside),
            // A presentation is no row, a section or a swipe row's content that is one included
            Place::Rows | Place::SectionRows | Place::Swipe(_) if presentation(tag, attrs) => {
                (false, Place::Outside)
            }
            Place::Rows if tag == "section" => (false, Place::SectionRows),
            Place::SectionRows if tag == "header" || tag == "footer" => (false, Place::Outside),
            Place::Rows | Place::SectionRows => match swipe_content(attrs) {
                Some(name) => (false, Place::Swipe(name)),
                None => (chevron, Place::Row(texts(v))),
            },
            Place::Swipe(name)
                if attrs.iter().any(|a| {
                    a.name == "id" && crate::swipe_rows::swipe_name(&a.value) == *name
                }) =>
            {
                (chevron, Place::Row(texts(v)))
            }
            Place::Swipe(name) => (false, Place::Swipe(name.clone())),
            Place::Row(_) if chevron => (true, Place::Outside),
            Place::Row(_) if presentation(tag, attrs) => (false, Place::Outside),
            // A control inside a row carries the row when it holds every text the row has; one that holds
            // none (a Next button, an expander) or not all (a labelled More beside the row's title) owns its glyphs
            Place::Row(row) if pressable(tag, attrs) => {
                let inside = texts(v);
                (
                    false,
                    if inside > 0 && inside == *row {
                        Place::Row(*row)
                    } else {
                        Place::Outside
                    },
                )
            }
            Place::Row(row) => (false, Place::Row(*row)),
        };
        let authored = attrs.iter().find(|a| a.name == AUTHORED);
        let written_mark = attrs
            .iter()
            .find(|a| a.name == MARK && a.written != IMPLIED)
            .map(|a| a.span);
        if marked || chevron || authored.is_some() || written_mark.is_some() {
            let one = seen
                .entry((v.span(), v.instance().unwrap_or_default()))
                .or_default();
            one.tag = tag.to_owned();
            one.marked |= marked;
            one.artwork |= chevron && !marked;
            one.authored = one.authored.take().or_else(|| authored.cloned());
            one.written_mark = one.written_mark.or(written_mark);
        }
        below
    });
    walk.for_each(drop);
    let mut errors = Vec::new();
    let mut marks = HashMap::new();
    let mut identities: Vec<_> = seen.into_iter().collect();
    identities
        .sort_by_key(|((span, instance), _)| (span.source_id, span.line, span.col, *instance));
    for (key, one) in identities {
        if one.marked && !one.artwork {
            marks.insert(
                key,
                one.authored
                    .as_ref()
                    .map_or("semantic", |a| mode(a, &mut errors)),
            );
        } else if let Some(a) = &one.authored {
            errors.push(LowerError {
                id: "lower-disclosure-placement",
                message: format!(
                    "`disclosure` is on this `{}`, which is not a row's disclosure. Remove the attribute, or, if this row's chevron should show on every host, move it to the row's literal `image \"symbol:forward-chevron\"`.",
                    one.tag
                ),
                span: a.span,
            });
        }
        if let Some(span) = one.written_mark {
            errors.push(LowerError {
                id: "lower-disclosure-placement",
                message: "`rowDisclosure` is Contract's mark on a row's chevron, not an attribute to write. Remove it; write `disclosure=\"always\"` on the row's `image \"symbol:forward-chevron\"` to show it on every host.".into(),
                span,
            });
        }
    }
    MARKS.set(marks);
    errors
}

/// How many texts the element `v` holds, itself included, within the row: not
/// in a presentation or a nested list, which hold their own.
fn texts(v: Visit<'_>) -> usize {
    let walk = v.subtree().walk_with(false, |n, &beyond| {
        beyond
            || n.element()
                .is_some_and(|(tag, _, attrs)| tag == "list" || presentation(tag, attrs))
    });
    walk.filter(|(n, beyond)| {
        !*beyond
            && n.element()
                .is_some_and(|(tag, _, _)| crate::prose::text_container(tag))
    })
    .count()
}

/// The content a swipe row names (`swipeContent`), compared as lowering
/// compares it.
fn swipe_content(attrs: &[Attr]) -> Option<Expr> {
    attrs
        .iter()
        .rev()
        .find(|a| a.name == "swipeContent")
        .map(|a| crate::swipe_rows::swipe_name(&a.value))
}

/// A mark's mode: `always` where its author wrote `disclosure="always"`.
fn mode(written: &Attr, errors: &mut Vec<LowerError>) -> &'static str {
    match literal(&written.value) {
        Expr::Str(value, _) if value == "always" => return "always",
        Expr::Str(value, span) => errors.push(LowerError {
            id: "lower-disclosure-value",
            message: format!(
                "`disclosure=\"{value}\"` is not a value. Omit the attribute for the platform's behaviour, write `disclosure=\"always\"` to show the chevron on every host, or remove the image to show none."
            ),
            span: *span,
        }),
        other => errors.push(LowerError {
            id: "lower-disclosure-value",
            message: "`disclosure` takes the literal string `\"always\"`, not another value or one computed when the page runs. Write `disclosure=\"always\"`, or omit it for the platform's behaviour.".into(),
            span: other.span(),
        }),
    }
    "semantic"
}

/// The mode of the mark the element at `span` in `instance` is, if it is one.
pub(crate) fn row_disclosure(span: Span, instance: u32) -> Option<&'static str> {
    MARKS.with_borrow(|marks| marks.get(&(span, instance)).copied())
}

/// A mark's attributes: its author's `disclosure` read into the mark.
pub(crate) fn marked_attrs(attrs: &[Attr], mode: &'static str, span: Span) -> Vec<Attr> {
    let mut out: Vec<Attr> = attrs
        .iter()
        .filter(|a| a.name != AUTHORED)
        .cloned()
        .collect();
    out.push(Attr {
        name: MARK.into(),
        value: Expr::Str(mode.into(), span),
        span,
        written: IMPLIED,
    });
    out
}

/// An element's attributes without a `disclosure` that is not on a mark, which
/// `find_row_disclosures` refused, so it is reported once.
pub(crate) fn without_misplaced(attrs: &[Attr]) -> Option<Vec<Attr>> {
    attrs.iter().any(|a| a.name == AUTHORED).then(|| {
        attrs
            .iter()
            .filter(|a| a.name != AUTHORED)
            .cloned()
            .collect()
    })
}

/// A mark's source, the portable role whatever spelling its author used (LLP 1035.004.000 D4).
pub(crate) fn marked_source(positional: &[Expr]) -> Vec<Expr> {
    let span = positional.first().map_or_else(Span::default, Expr::span);
    std::iter::once(Expr::Str("symbol:forward-chevron".into(), span))
        .chain(positional.iter().skip(1).cloned())
        .collect()
}

/// A popover, menu or dialog, by its tag or its `popover` attribute, or a field.
fn presentation(tag: &str, attrs: &[Attr]) -> bool {
    matches!(tag, "popover" | "menu" | "dialog" | "select" | "input")
        || attrs.iter().any(|a| a.name == "popover")
}

fn pressable(tag: &str, attrs: &[Attr]) -> bool {
    matches!(tag, "button" | "link") || attrs.iter().any(|a| a.name == "press" || a.name == "href")
}

/// Whether an element is an image of a literal forward chevron, in any of its
/// spellings (LLP 1035.004), that no icon the app declares resolves and that
/// has no press of its own.
fn literal_chevron(tag: &str, positional: &[Expr], attrs: &[Attr]) -> bool {
    if tag != "image" || pressable(tag, attrs) {
        return false;
    }
    let Some(Expr::Str(source, _)) = positional.first().map(literal) else {
        return false;
    };
    let Some(name) = source.strip_prefix("symbol:") else {
        return false;
    };
    if crate::icons::named(name).is_some() {
        return false;
    }
    let role = crate::icons::role_or_accessory(name).unwrap_or(name);
    // The role's portable aliases resolve to it; its `sf/` names are matched as written
    matches!(
        role,
        "forward-chevron" | "sf/chevron.forward" | "sf/chevron.right"
    )
}

fn literal(expr: &Expr) -> &Expr {
    let mut value = expr;
    while let Expr::Typed(inner, _, _) = value {
        value = inner;
    }
    value
}
