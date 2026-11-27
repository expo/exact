//! A text field's selection, and the `InputEvent` record (x2apps codeedit
//! #2, survey #2): what `input` and `change` hand an action that takes one
//! more parameter, and what a text field's `select` carries — the target's
//! own fields as the event fires, by the DOM's names
//! (`contract/types/src/selection.rs` declares the record).

use super::{ControlValue, DataSource, Event, Runner};
use exact_kernel::{ControlKind, PropId, ViewId};
use exact_plan::Value;

/// A text field's selection as the DOM reports it: `selectionStart`,
/// `selectionEnd` in UTF-16 code units of its value, and
/// `selectionDirection`; a caret is a selection whose ends are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSelection {
    /// `selectionStart`.
    pub start: u32,
    /// `selectionEnd`, never before `start`.
    pub end: u32,
    /// `selectionDirection`.
    pub direction: SelectionDirection,
}

/// HTML's `selectionDirection`: which end of a selection the person moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionDirection {
    /// `forward`: the end moves.
    Forward,
    /// `backward`: the start moves.
    Backward,
    /// `none`: the platform keeps no direction (a caret).
    #[default]
    None,
}

impl SelectionDirection {
    /// The DOM's word.
    pub fn name(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
            Self::None => "none",
        }
    }

    /// The direction a word names, as `setSelectionRange` reads its third
    /// argument: anything but `forward` or `backward` is `none`.
    pub fn of(word: &str) -> Self {
        match word {
            "forward" => Self::Forward,
            "backward" => Self::Backward,
            _ => Self::None,
        }
    }
}

impl FieldSelection {
    /// A caret at `at`.
    pub fn caret(at: u32) -> Self {
        Self {
            start: at,
            end: at,
            direction: SelectionDirection::None,
        }
    }

    /// The caret after the last character of `text`: where an edit a host
    /// reports no selection for leaves it (typing at the end).
    pub fn at_end(text: &str) -> Self {
        Self::caret(utf16_len(text))
    }

    /// `setSelectionRange(start, end, direction)` on a value of `len` UTF-16
    /// units, as HTML clamps it: each end an `unsigned long` (WebIDL's
    /// ToUint32, so `-1` is past the end), then to the length, and an end
    /// before the start moves the start to it.
    pub fn clamped(start: f64, end: f64, direction: SelectionDirection, len: u32) -> Self {
        let at = |n: f64| {
            let n = if n.is_finite() {
                (n.trunc() % 4_294_967_296.0 + 4_294_967_296.0) % 4_294_967_296.0
            } else {
                0.0
            };
            n.min(f64::from(len)) as u32
        };
        let end = at(end);
        Self {
            start: at(start).min(end),
            end,
            direction,
        }
    }

    /// Decode `start,end,direction,` then the field's text verbatim: what
    /// ABI kinds 40 to 42 carry. The offsets are whole, `start <= end`, each
    /// within the text's UTF-16 length, the direction a DOM word.
    pub fn parse(payload: &str) -> Option<(Self, &str)> {
        let mut parts = payload.splitn(4, ',');
        let start: u32 = parts.next()?.parse().ok()?;
        let end: u32 = parts.next()?.parse().ok()?;
        let direction = match parts.next()? {
            "forward" => SelectionDirection::Forward,
            "backward" => SelectionDirection::Backward,
            "none" => SelectionDirection::None,
            _ => return None,
        };
        let text = parts.next()?;
        (start <= end && end <= utf16_len(text)).then_some((
            Self {
                start,
                end,
                direction,
            },
            text,
        ))
    }
}

/// A string's length in UTF-16 code units, the DOM's offsets' unit.
pub fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count().try_into().unwrap_or(u32::MAX)
}

/// The edit's DOM metadata, shared by beforeinput and input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputDetails {
    /// DOM inputType, such as insertText or historyUndo.
    pub input_type: String,
    /// The inserted text; absent for deletion and history.
    pub data: Option<String>,
    /// Whether preventDefault can stop this edit.
    pub cancelable: bool,
    /// Whether this edit is part of an IME composition.
    pub is_composing: bool,
}

impl Event {
    /// ABI 53 (beforeinput) and 54 (input): type, cancelable, composing,
    /// UTF-8 data length (-1 for null), data, then the field selection payload.
    pub fn edit_payload(kind: u32, payload: &str) -> Option<Self> {
        let mut parts = payload.splitn(5, ',');
        let input_type = parts.next()?.to_owned();
        let flag = |s| match s {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        };
        let cancelable = flag(parts.next()?)?;
        let is_composing = flag(parts.next()?)?;
        let len: i64 = parts.next()?.parse().ok()?;
        let rest = parts.next()?;
        let (data, rest) = if len == -1 {
            (None, rest)
        } else {
            let len = usize::try_from(len).ok()?;
            (Some(rest.get(..len)?.to_owned()), rest.get(len..)?)
        };
        let (selection, text) = FieldSelection::parse(rest)?;
        let edit = InputDetails {
            input_type,
            data,
            cancelable,
            is_composing,
        };
        match kind {
            53 => Some(Self::BeforeInput(text.into(), selection, edit)),
            54 => Some(Self::Input(ControlValue::Edit(
                text.into(),
                selection,
                edit,
            ))),
            _ => None,
        }
    }

    /// Decode ABI kind 40 (a text field's `input`), 41 (its `change`) or 42
    /// (its `select`), or 48 (a scripted replacement input), each carrying its selection then its text
    /// ([`FieldSelection::parse`]).
    pub fn field_payload(kind: u32, payload: &str) -> Option<Self> {
        let (selection, text) = FieldSelection::parse(payload)?;
        let value = ControlValue::Field(text.into(), selection);
        match kind {
            48 => Some(Event::Input(ControlValue::Replacement(
                text.into(),
                selection,
            ))),
            40 => Some(Event::Input(value)),
            41 => Some(Event::Change(value)),
            42 => Some(Event::FieldSelect(text.into(), selection)),
            52 => Some(Event::FieldSelectionChange(text.into(), selection)),
            _ => None,
        }
    }
}

impl<D: DataSource> Runner<D> {
    /// The `InputEvent` an `input`, `change` or text field's `select` at
    /// `view` hands its action, its fields in the compiler's order: the
    /// value the event carries (a checkbox's or a radio's own `value`, `on`
    /// when it has none; a range's number written as JavaScript would),
    /// whether it is checked, and a text field's selection (where a host
    /// reported none, the caret after its text, as typing leaves it); a
    /// control without a text selection reports 0, 0 and `none`.
    pub(super) fn input_record(
        &self,
        view: ViewId,
        payload: &Value,
        reported: Option<FieldSelection>,
        edit: Option<&InputDetails>,
        replacement: bool,
    ) -> Value {
        let node = self.kernel.node(view);
        let kind = node.and_then(|n| ControlKind::of(n.node_type, n.props));
        let own = || {
            node.and_then(|n| n.props.str(PropId::Value))
                .unwrap_or("on")
                .to_owned()
        };
        let (value, checked) = match (kind, payload) {
            (Some(ControlKind::Checkbox | ControlKind::Switch), Value::Bool(on)) => (own(), *on),
            (Some(ControlKind::Radio), _) => (own(), true),
            (_, Value::Number(n)) => (exact_num::Shortest(*n).to_string(), false),
            (_, v @ exact_plan::str_value!()) => (v.text().to_owned(), false),
            _ => (String::new(), false),
        };
        let selection = match kind {
            None => Some(reported.unwrap_or_else(|| FieldSelection::at_end(&value))),
            Some(_) => None,
        };
        let (start, end, direction) = selection.map_or((0, 0, SelectionDirection::None), |s| {
            (s.start, s.end, s.direction)
        });
        Value::record(vec![
            Value::str(&value),
            Value::Bool(checked),
            Value::Number(f64::from(start)),
            Value::Number(f64::from(end)),
            Value::str(direction.name()),
            Value::str(if let Some(edit) = edit {
                &edit.input_type
            } else if replacement {
                "insertReplacementText"
            } else {
                ""
            }),
            edit.and_then(|e| e.data.as_ref())
                .map_or(Value::NONE, |s| Value::some(Value::str(s))),
            Value::Bool(edit.is_some_and(|e| e.cancelable)),
            Value::Bool(edit.is_some_and(|e| e.is_composing)),
        ])
    }
}

impl FieldSelection {
    /// Carry both UTF-16 offsets through the changed middle (LLP 1102 §3.16).
    pub fn through_rewrite(self, typed: &str, accepted: &str) -> Self {
        let a: Vec<_> = typed.encode_utf16().collect();
        let b: Vec<_> = accepted.encode_utf16().collect();
        let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let suffix = a[prefix..]
            .iter()
            .rev()
            .zip(b[prefix..].iter().rev())
            .take_while(|(x, y)| x == y)
            .count();
        let old_end = (a.len() - suffix) as u32;
        let new_end = (b.len() - suffix) as u32;
        let prefix = prefix as u32;
        let carry = |p: u32| {
            if p <= prefix {
                p
            } else if p < old_end {
                p.min(new_end)
            } else {
                new_end + (p - old_end)
            }
        };
        Self {
            start: carry(self.start),
            end: carry(self.end),
            ..self
        }
    }
}

pub(super) struct InputEdit {
    view: ViewId,
    node: exact_plan::NodesId,
    typed: Option<Value>,
}

impl<D: DataSource> Runner<D> {
    pub(super) fn run_input(
        &mut self,
        view: ViewId,
        node: exact_plan::NodesId,
        handler: &exact_plan::HandlersRow,
        args: Vec<Value>,
        frames: &[crate::vm::Frame],
        composing: bool,
    ) -> Result<exact_kernel::CommitReceipt, super::RunnerError> {
        let edit = args
            .get(handler.args.len as usize)
            .filter(|v| v.is_str())
            .map(|v| InputEdit {
                view,
                node,
                typed: (!composing).then(|| v.clone()),
            });
        let was = self.poisoned;
        let result = self.run_action_from(handler.action, args, frames, edit);
        self.record_ran(
            "handler",
            self.plan.action(handler.action).name,
            was,
            &result,
        );
        result
    }

    /// Only an equal binding needs a value op that the normal prop diff cannot emit.
    pub(super) fn reconciled_input(
        &self,
        edit: InputEdit,
        frames: &[crate::vm::Frame],
        writes: &crate::vm::Outcome,
    ) -> Option<(exact_kernel::NodeKey, String)> {
        if writes.writes.is_empty() && writes.row_writes.is_empty() {
            return None;
        }
        let node = self.kernel.node(edit.view)?;
        if node.node_type != exact_kernel::NodeType::TextInput
            || !matches!(
                node.props.str(PropId::Type).unwrap_or("text"),
                "text" | "search" | "email" | "tel" | "url" | "password"
            )
        {
            return None;
        }
        let old = exact_kernel::control::bound_field_value(node.props);
        if edit.typed.as_ref().and_then(Value::as_str) == Some(old.as_ref()) {
            return None;
        }
        let binding = self
            .plan
            .node(edit.node)
            .bindings
            .iter()
            .map(|b| self.plan.binding(b))
            .find(|b| b.kind == exact_plan::BindingKind::Prop && b.id == PropId::Value as u16)?;
        let env = self.env(&[], frames);
        // A direct echo needs no value op. During composition an equal op can
        // clear the widget's previously held rewrite without replacing its text.
        let (code, read_frames) =
            self.plan
                .read_code_in(self.plan.code(binding.expr), frames, |f| f.region)?;
        if let [op, a, b, c, d, ret] = code {
            if *op == exact_plan::Opcode::LoadSlot as u8 && *ret == exact_plan::Opcode::Return as u8
            {
                let slot = u32::from_le_bytes([*a, *b, *c, *d]);
                let written =
                    if let Some(owner) = self.plan.owner_region(&self.plan.slots[slot as usize]) {
                        let row = crate::vm::Frame::row_of(read_frames, owner.0)?;
                        writes
                            .row_writes
                            .iter()
                            .rev()
                            .find(|(s, _, r)| *s == slot && std::rc::Rc::ptr_eq(r, row))
                            .map(|(_, v, _)| v)
                    } else {
                        writes
                            .writes
                            .iter()
                            .rev()
                            .find(|(s, _)| *s == slot)
                            .map(|(_, v)| v)
                    }?;
                return (edit.typed.as_ref() != Some(written))
                    .then(|| (node.key, old.into_owned()));
            }
        }
        reads_assignment(&env, binding.expr, writes).then(|| (node.key, old.into_owned()))
    }
}

fn reads_assignment(
    env: &crate::vm::Env<'_>,
    expr: exact_plan::Code,
    writes: &crate::vm::Outcome,
) -> bool {
    use crate::vm::{observe_reads, Frame};
    use exact_plan::Opcode;
    use std::{
        cell::{Cell, RefCell},
        collections::HashSet,
        rc::Rc,
    };

    let found = Cell::new(false);
    let derives = RefCell::new(Vec::new());
    let observe = |here: &crate::vm::Env<'_>, ins: &crate::vm::Instruction| {
        let slot = ins.args[0] as u32;
        match ins.op {
            Opcode::LoadSlot => {
                if writes.writes.iter().any(|(s, _)| *s == slot)
                    || here
                        .plan
                        .owner_region(&here.plan.slots[slot as usize])
                        .and_then(|owner| Frame::row_of(here.frames, owner.0))
                        .is_some_and(|row| {
                            writes
                                .row_writes
                                .iter()
                                .any(|(s, _, r)| *s == slot && Rc::ptr_eq(r, row))
                        })
                {
                    found.set(true);
                }
            }
            Opcode::LoadDerive if here.plan.unsettled_body(slot).is_none() => {
                derives.borrow_mut().push(slot)
            }
            _ => {}
        }
    };
    observe_reads(env.plan.code(expr), env, &observe);
    let mut seen = HashSet::new();
    while !found.get() {
        let Some(id) = derives.borrow_mut().pop() else {
            break;
        };
        if seen.insert(id) {
            observe_reads(
                env.plan.code(env.plan.derives[id as usize].body),
                &crate::vm::Env {
                    frames: &[],
                    ..*env
                },
                &observe,
            );
        }
    }
    found.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_suffix_can_include_one_surrogate() {
        assert_eq!(
            FieldSelection::caret(2).through_rewrite("abc\u{10000}", "\u{10400}"),
            FieldSelection::caret(1)
        );
    }

    #[test]
    fn an_input_receipt_names_only_the_editor_whose_action_assigned_a_read_slot() {
        let plan = contract::compile(
            r#"shape Form
  name: string
  count: number

component App
  state form = Form(name="A", count=0)
  state other = "A"
  state enabled = false
  derive value = enabled ? other : form.name
  action update(v: string)
    form = Form(form, count=form.count + 1)
  action validate(v: string)
    other = v
  action flip(v: string)
    enabled = true
  action guarded(v: string)
    if v == "ok"
      other = v
  view
    column
      Child(term=value, edit=update)
      input value=value input=validate testId="observe"
      input value=value input=flip testId="switch"
      input value=value input=guarded testId="guard"
      input input=update testId="free"

component Child
  props
    term: string
    edit: action
  action write(v: string)
    edit(v)
  view
    input value=term input=write testId="child"
"#,
        )
        .unwrap();
        let mut runner = Runner::boot(
            plan,
            (),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        for (field, text, reconciled) in [
            ("child", "denied", true),
            ("free", "raw", false),
            ("observe", "A", false),
            ("switch", "switch", true),
            ("child", "denied", false),
            ("guard", "ignored", false),
        ] {
            let key = runner.kernel().find_first_by_test_id(field).unwrap();
            let id = runner.kernel().node_by_key(key).unwrap().id;
            let receipt = runner.dispatch(id, Event::Input(text.into())).unwrap();
            assert_eq!(
                receipt.reconcile_field,
                reconciled.then_some(key),
                "{field}"
            );
        }
    }

    #[test]
    fn edit_payload_keeps_null_empty_and_utf8_data_distinct() {
        for (payload, expected) in [
            ("insertText,1,0,0,0,0,none,", Some("")),
            ("historyUndo,1,0,-1,0,0,none,", None),
            ("insertText,1,0,7,a,\n😀0,0,none,", Some("a,\n😀")),
        ] {
            let Event::BeforeInput(_, _, edit) = Event::of_host_kind(53, payload).unwrap() else {
                panic!("beforeinput")
            };
            assert_eq!(edit.data.as_deref(), expected);
        }
        for payload in [
            "insertText,2,0,0,0,0,none,",
            "insertText,1,0,-2,0,0,none,",
            "insertText,1,0,1,😀0,0,none,",
            "insertText,1,0,0,2,1,none,abc",
        ] {
            assert!(Event::of_host_kind(53, payload).is_err(), "{payload}");
        }
    }

    #[test]
    fn replacement_input_reports_its_type_and_selection() {
        let event = Event::of_host_kind(48, "1,3,none,aXYb").unwrap();
        assert!(
            matches!(event, Event::Input(ControlValue::Replacement(text, s))
            if text == "aXYb" && s.start == 1 && s.end == 3)
        );
        assert!(Event::of_host_kind(48, "3,1,none,abc").is_err());
    }

    #[test]
    fn a_fields_selection_decodes_and_clamps_as_htmls() {
        let (s, text) = FieldSelection::parse("1,3,backward,a,b\nc").unwrap();
        assert_eq!(
            (s.start, s.end, s.direction),
            (1, 3, SelectionDirection::Backward)
        );
        assert_eq!(text, "a,b\nc");
        // Offsets are UTF-16: an emoji is two.
        assert!(FieldSelection::parse("0,2,none,😀").is_some());
        for bad in [
            "3,1,none,abcd",
            "0,9,none,abc",
            "0,0,sideways,",
            "0,0,none",
            "x,0,none,",
        ] {
            assert!(FieldSelection::parse(bad).is_none(), "{bad}");
        }
        assert!(matches!(
            Event::field_payload(42, "0,1,forward,ab"),
            Some(Event::FieldSelect(text, s)) if text == "ab" && s.end == 1
        ));
        assert!(matches!(
            Event::field_payload(40, "2,2,none,ab"),
            Some(Event::Input(ControlValue::Field(text, s))) if text == "ab" && s.start == 2
        ));
        let c = FieldSelection::clamped(9.0, 4.0, SelectionDirection::of("sideways"), 6);
        assert_eq!(
            (c.start, c.end, c.direction),
            (4, 4, SelectionDirection::None)
        );
        let c = FieldSelection::clamped(0.5, 99.0, SelectionDirection::of("forward"), 6);
        assert_eq!(
            (c.start, c.end, c.direction),
            (0, 6, SelectionDirection::Forward)
        );
        // As the DOM converts it, `-1` is the largest offset: the end.
        let c = FieldSelection::clamped(-1.0, -1.0, SelectionDirection::None, 6);
        assert_eq!((c.start, c.end), (6, 6));
        let c = FieldSelection::clamped(f64::NAN, f64::INFINITY, SelectionDirection::None, 6);
        assert_eq!((c.start, c.end), (0, 0));
        assert_eq!(FieldSelection::at_end("a😀").end, 3);
    }
}
