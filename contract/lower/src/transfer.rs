//! @ref LLP 1094.000 D1, D2, D8, D11 — a drag out of the app: `draggable`,
//! HTML's attribute, and `dataTransfer=DataTransfer(text=, url=, files=)`,
//! what the drag carries, bound to props the host holds before the gesture.
//! The type checker reads the arguments; this binds them and refuses what a
//! node cannot be: a draggable with nothing to carry, data with no drag, a
//! node that has a drag or gesture of its own or is a run of text, a gesture
//! that claims the same movement, and anything inside a grouped list.
use crate::{err, expr, tags, LowerError, Lowerer};
use contract_syntax::{Attr, Expr};
use contract_types::{Scope, Ty};
use exact_kernel::{NodeType, PropId};
use exact_plan::{asm::Asm, BindingsRow, Stdlib};

/// Gestures that claim the movement a drag's lift does, on one node (D8).
const GESTURES: [&str; 5] = [
    "pan",
    "panrelease",
    "reorderFor",
    "transformDragFor",
    "heightDragFor",
];

/// Tags whose node type would take a drag but whose element has a drag,
/// a gesture or a presentation of its own.
const REFUSED_BOXES: [&str; 2] = ["main", "dialog"];

impl Lowerer<'_> {
    /// `dataTransfer=DataTransfer(…)`: each argument's own expression bound
    /// to its prop, `files` joined by newlines as `midiinputs=` is.
    pub(crate) fn data_transfer(
        &mut self,
        a: &Attr,
        scope: &Scope,
        locals: u16,
        bindings: &mut Vec<BindingsRow>,
    ) -> Result<(), LowerError> {
        let Expr::Call(_, args, _) = &a.value else {
            return err(
                "type-data-transfer",
                "`dataTransfer` takes `DataTransfer(text=…, url=…, files=[…])`",
                a.span,
            );
        };
        for arg in args {
            let Expr::NamedArg(name, value, _) = arg else {
                continue;
            };
            let prop = match name.as_str() {
                "text" => PropId::DataTransferText,
                "url" => PropId::DataTransferUrl,
                _ => PropId::DataTransferFiles,
            };
            let (mut asm, mut depth) = (Asm::new(), locals);
            let ty = expr::compile(self, &mut asm, value, scope, &mut depth)?;
            if matches!(&ty, Ty::List(_)) {
                asm.str(self.b.str("\n"));
                asm.call(Stdlib::Join);
            }
            let code = self.b.code(asm);
            bindings.push(crate::prop_row(prop, code, a.written));
        }
        Ok(())
    }
}

/// The node's own rules: where `draggable` goes, that it carries something,
/// that `dataTransfer` drags, and which gestures it may share a node with.
pub(crate) fn check_node(
    tag: &str,
    parent_tag: Option<&str>,
    attrs: &[Attr],
) -> Result<(), LowerError> {
    let find = |name: &str| attrs.iter().find(|a| a.name == name);
    let (draggable, data) = (find("draggable"), find("dataTransfer"));
    if let Some(d) = draggable {
        let run = parent_tag.is_some_and(crate::prose::text_container);
        let boxed = tags::tag(tag).is_some_and(|t| {
            matches!(
                t.node_type,
                NodeType::View | NodeType::Pressable | NodeType::Image
            )
        }) && !REFUSED_BOXES.contains(&tag)
            && !run;
        if !boxed {
            let why = if run {
                "a run of text is part of its paragraph; mark the box around the text"
            } else {
                own_drag(tag)
            };
            return err(
                "lower-attr-tag",
                format!("`draggable` belongs to a box, a `link`, a `button` or an `image`, not `{tag}`: {why}"),
                d.span,
            );
        }
        // `draggable=false` drags nothing, so it needs no data and claims no movement.
        if never(d) {
            return Ok(());
        }
        if data.is_none() {
            let what = if tag == "image" {
                "say what it carries: `dataTransfer=DataTransfer(files=[…])` with the image's own file, or `url=` for a remote one"
            } else {
                "add `dataTransfer=DataTransfer(files=[…])`, or `text=` or `url=`"
            };
            return err(
                "lower-draggable-data",
                format!("`draggable=true` on this `{tag}` carries nothing. A drag's data is fixed before the gesture begins, so it is declared on the node: {what}"),
                d.span,
            );
        }
        if let Some(g) = attrs.iter().find(|a| GESTURES.contains(&a.name.as_str())) {
            return err(
                "lower-draggable-gesture",
                format!("`draggable` and `{}` on one node claim the same movement, and the platforms disagree about which wins. Put `{}` on a child, or `draggable` on a wrapper", g.name, g.name),
                g.span,
            );
        }
    }
    match (data, draggable) {
        (Some(a), None) => err(
            "lower-attr-pair",
            format!("`dataTransfer` says what a drag carries, and this `{tag}` never drags. Add `draggable=true`, or a boolean binding that turns the drag on"),
            a.span,
        ),
        _ => Ok(()),
    }
}

/// A literal `draggable=false`.
fn never(a: &Attr) -> bool {
    matches!(a.value, Expr::Bool(false, _))
}

/// Why a tag that refuses `draggable` does: what it has of its own.
fn own_drag(tag: &str) -> &'static str {
    match tag {
        "input" | "textarea" => "a text field drags its own selected text",
        "scroll" | "list" => "a scroller's drags scroll it; mark its rows",
        "main" | "dialog" => "it is the page's or a dialog's frame, not something a person lifts",
        "text" => "mark the box around the text",
        _ => "its element has a drag, a gesture or a presentation of its own",
    }
}

/// Whether `a` can make its node drag: a `draggable` but a literal `false`.
pub(crate) fn drags(a: &Attr) -> bool {
    a.name == "draggable" && !never(a)
}

/// The refusal of a `draggable` inside a grouped list's section: a row the
/// host reads as data has no view, and the others are the platform's cells
/// (LLP 1094.000 D4).
pub(crate) fn grouped(a: &Attr) -> LowerError {
    LowerError {
        id: "lower-draggable-grouped",
        message: "A grouped list's rows are the platform's own cells, and nothing inside them drags out yet. Put what should drag in a plain `list` or a `column`, or offer Share… in the row's menu".into(),
        span: a.span,
    }
}

/// `dragstart`, `drag` and `dragend`, which this release does not hear.
pub(crate) fn handler_later(a: &Attr) -> LowerError {
    LowerError {
        id: "lower-handler-later",
        message: format!("`{}` is not heard yet. Declare the data with `dataTransfer=`; a drag's start and end events come later", a.name),
        span: a.span,
    }
}
