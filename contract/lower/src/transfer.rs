//! @ref LLP 1094.000 D1, D2, D8, D11, D14 — a drag out of the app:
//! `draggable`, HTML's attribute, and `dataTransfer=DataTransfer(text=, url=,
//! files=, names=)`, what the drag carries, bound to props the host holds
//! before the gesture.
//! The type checker reads the arguments; this binds them and refuses what a
//! node cannot be: a draggable with nothing to carry, data with no drag, a
//! node that has a drag or gesture of its own or is a run of text, a gesture
//! that claims the same movement, and anything inside a grouped list.
use crate::{err, expr, tags, LowerError, Lowerer};
use contract_syntax::{Attr, Expr};
use contract_types::{Scope, Ty};
use exact_kernel::{NodeType, PropId};
use exact_plan::{asm::Asm, BindingsRow, Opcode, Stdlib};

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
    /// `dataTransfer=DataTransfer(…)`: `text` and `url` each bound to its
    /// prop, and `files`, with `names` when given, to `dataTransferEntries`
    /// as D14 encodes them.
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
        let named = |want: &str| {
            args.iter().find_map(|arg| match arg {
                Expr::NamedArg(name, value, _) if name == want => Some(&**value),
                _ => None,
            })
        };
        // In the order written, so a commit journals refusals in that order
        for arg in args {
            let Expr::NamedArg(name, value, _) = arg else {
                continue;
            };
            match name.as_str() {
                "text" => self.data_transfer_row(
                    PropId::DataTransferText,
                    value,
                    scope,
                    locals,
                    a.written,
                    bindings,
                )?,
                "url" => self.data_transfer_row(
                    PropId::DataTransferUrl,
                    value,
                    scope,
                    locals,
                    a.written,
                    bindings,
                )?,
                "files" => {
                    let code = self.drag_entries(value, named("names"), scope, locals)?;
                    bindings.push(crate::prop_row(
                        PropId::DataTransferEntries,
                        code,
                        a.written,
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn data_transfer_row(
        &mut self,
        prop: PropId,
        value: &Expr,
        scope: &Scope,
        locals: u16,
        written: bool,
        bindings: &mut Vec<BindingsRow>,
    ) -> Result<(), LowerError> {
        let (mut asm, mut depth) = (Asm::new(), locals);
        let ty = expr::compile(self, &mut asm, value, scope, &mut depth)?;
        debug_assert!(!matches!(&ty, Ty::List(_)), "each argument binds a string");
        let code = self.b.code(asm);
        bindings.push(crate::prop_row(prop, code, written));
        Ok(())
    }
}

impl Lowerer<'_> {
    /// `files` and `names` as one string (LLP 1094.000 D14): each file
    /// percent-encoded and followed by a line feed, then, with `names`,
    /// U+001E and each name likewise, so no line break in a bound value can
    /// shift an entry and an empty name keeps its place. The plan's own
    /// operations, not calls by name, so an app's `fn map` or `fn join`
    /// cannot stand in for them.
    fn drag_entries(
        &mut self,
        files: &Expr,
        names: Option<&Expr>,
        scope: &Scope,
        locals: u16,
    ) -> Result<exact_plan::Code, LowerError> {
        let (mut asm, mut depth) = (Asm::new(), locals);
        self.drag_encoded(&mut asm, files, scope, &mut depth)?;
        if let Some(names) = names {
            asm.str(self.b.str(&exact_plan::DRAG_NAMES_MARK.to_string()));
            asm.simple(Opcode::Concat);
            self.drag_encoded(&mut asm, names, scope, &mut depth)?;
            asm.simple(Opcode::Concat);
        }
        Ok(self.b.code(asm))
    }

    /// `list`, each entry percent-encoded and followed by a line feed, joined.
    fn drag_encoded(
        &mut self,
        asm: &mut Asm,
        list: &Expr,
        scope: &Scope,
        locals: &mut u16,
    ) -> Result<(), LowerError> {
        expr::compile(self, asm, list, scope, locals)?;
        // The VM binds each item and its index to the next two locals (`expr::callback_body`)
        let (end, item) = (asm.label(), *locals);
        asm.each_item(Opcode::Map, end);
        asm.load_local(item);
        asm.call(Stdlib::EncodeURIComponent);
        asm.str(self.b.str("\n"));
        asm.simple(Opcode::Concat);
        asm.place(end);
        asm.str(self.b.str(""));
        asm.call(Stdlib::Join);
        Ok(())
    }
}

/// A drag's attributes: DOM's `drop` of files (studio diary R19) and the
/// `dragenter` and `dragleave` of a drag over the node, then a drag of the
/// node itself (@ref LLP 1094.000 D1, D16): `draggable`, what it carries,
/// and its start and end.
pub(crate) fn attr(name: &str) -> Option<tags::AttrTarget> {
    use tags::AttrTarget::{DataTransfer, Handler, Prop};
    Some(match name {
        "drop" => Handler("drop"),
        "dragenter" => Handler("dragenter"),
        "dragleave" => Handler("dragleave"),
        "draggable" => Prop(tags::p("draggable")),
        "dataTransfer" => DataTransfer,
        "dragstart" => Handler("dragstart"),
        "dragend" => Handler("dragend"),
        _ => return None,
    })
}

/// The node's own rules: where `draggable` goes, that it carries something,
/// that `dataTransfer` drags, that a drag's start and end are heard where it
/// begins (LLP 1094.000 D16), and which gestures it may share a node with.
pub(crate) fn check_node(
    tag: &str,
    parent_tag: Option<&str>,
    attrs: &[Attr],
) -> Result<(), LowerError> {
    let find = |name: &str| attrs.iter().find(|a| a.name == name);
    let (draggable, data) = (find("draggable"), find("dataTransfer"));
    let source_event = attrs
        .iter()
        .find(|a| matches!(a.name.as_str(), "dragstart" | "dragend"));
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
            return match source_event {
                Some(h) => err(
                    "lower-drag-event-source",
                    format!("`{}` is heard where a drag begins, and this `{tag}` has `draggable=false`, so it never drags. Give `draggable` `true` or a boolean binding, or move the handler to the node that drags", h.name),
                    h.span,
                ),
                None => Ok(()),
            };
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
    if let (Some(h), None) = (source_event, draggable) {
        return err(
            "lower-drag-event-source",
            format!("`{}` is heard where a drag begins, on a node that declares `draggable` and its `dataTransfer`. Move the handler to the draggable node, or declare `draggable=true` and a `dataTransfer=` on this `{tag}`", h.name),
            h.span,
        );
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

/// `drag`, which no host reports at a drag's source (LLP 1094.000 D16).
pub(crate) fn handler_later(a: &Attr) -> LowerError {
    LowerError {
        id: "lower-handler-later",
        message: format!("`{}` is not heard: Exact does not expose a drag's continuous motion at its source. Hear `dragstart` and `dragend` for its start and end", a.name),
        span: a.span,
    }
}
