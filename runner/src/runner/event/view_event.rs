//! A package view's declared event (LLP 1133.021 §9 item 2): the
//! implementation sends a message whose text is `{"exact:event": name,
//! "args": [...]}`, and the lowering made each `name=action` a `message`
//! handler whose one bound argument is `exact:event:<name>`
//! (`contract_lower`'s `native_typed.rs`). The message runs the handler
//! that names it, with its arguments decoded by the action's parameter
//! types; a message that names no declared event runs the node's plain
//! `message` handler, if it has one.

use super::super::{DataSource, Runner, RunnerError};
use crate::vm::Frame;
use exact_kernel::CommitReceipt;
use exact_plan::{EventKind, HandlersRow, NodesId, Value};
use serde_json::Value as Json;

impl<D: DataSource> Runner<D> {
    /// `None` where node `node` has no declared event's handler, for the
    /// ordinary dispatch to take the message.
    pub(super) fn view_event(
        &mut self,
        view: exact_kernel::ViewId,
        node: NodesId,
        text: &str,
        frames: &[Frame],
        what: &mut String,
    ) -> Option<Result<CommitReceipt, RunnerError>> {
        let marker = "exact:event:";
        let mut marked: Vec<(String, HandlersRow)> = Vec::new();
        let mut plain = None;
        for h in self
            .plan
            .node(node)
            .handlers
            .iter()
            .map(|h| self.plan.handler(h).clone())
        {
            if h.event != EventKind::Message {
                continue;
            }
            let first = h.args.iter().next().map(|a| self.plan.arg(a).expr);
            let named = first
                .and_then(|code| self.eval(code, &[], frames).ok())
                .and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.strip_prefix(marker))
                        .map(str::to_owned)
                });
            match named {
                Some(event) => marked.push((event, h)),
                None if plain.is_none() => plain = Some(h),
                None => {}
            }
        }
        if marked.is_empty() {
            return None;
        }
        let sent: Json = serde_json::from_str(text).unwrap_or_default();
        let Some(event) = sent["exact:event"].as_str() else {
            let h = plain?;
            let mut args = Vec::new();
            for (k, a) in h.args.iter().enumerate() {
                match self
                    .eval(self.plan.arg(a).expr, &[], frames)
                    .map_err(|e| e.argument(view, node, "message", k))
                {
                    Ok(v) => args.push(v),
                    Err(e) => return Some(Err(e)),
                }
            }
            args.push(Value::str(text));
            return Some(self.run_action(h.action, args, frames));
        };
        let Some((_, h)) = marked.into_iter().find(|(e, _)| e == event) else {
            return Some(Ok(CommitReceipt::default()));
        };
        let action = self.plan.action(h.action).clone();
        let mut args = Vec::new();
        for (i, p) in action.params.iter().enumerate() {
            let ty = self.plan.param(p).ty;
            let json = sent["args"].get(i).cloned().unwrap_or(Json::Null);
            match self.event_arg(ty, &json) {
                Ok(v) => args.push(v),
                Err(why) => {
                    self.log(format!("{event}: refused: {why}"));
                    return Some(Ok(CommitReceipt::default()));
                }
            }
        }
        super::super::lines::ran(what, self.plan.str(action.name));
        Some(self.run_action(h.action, args, frames))
    }
}

impl<D: DataSource> Runner<D> {
    /// One argument as the action's parameter takes it.
    pub(in crate::runner) fn event_arg(
        &self,
        ty: exact_plan::TypesId,
        json: &Json,
    ) -> Result<Value, String> {
        let shape = exact_js_value::Shape::from_plan(&self.plan, ty)?;
        let mut json = json.clone();
        absent_is_none(&mut json, &shape);
        let value = exact_js_value::from_json(&json, &shape)?;
        if !value.conforms(&self.plan, ty) {
            return Err(crate::conform::mismatch(&self.plan, &value, ty));
        }
        Ok(value)
    }
}

/// A module's answer leaves out an option field it has nothing for (a
/// platform's words where another platform answers, a declared default):
/// absent is `none`, as an omitted word is (LLP 1133.002 D2).
pub(in crate::runner) fn absent_is_none(json: &mut Json, shape: &exact_js_value::Shape) {
    use exact_js_value::Shape;
    match (json, shape) {
        (Json::Object(map), Shape::Record(fields)) => {
            for (name, field) in fields {
                match map.get_mut(name) {
                    Some(value) => absent_is_none(value, field),
                    None if matches!(field, Shape::Option(_)) => {
                        map.insert(name.clone(), Json::Null);
                    }
                    None => {}
                }
            }
        }
        (Json::Array(items), Shape::List(item)) => {
            items.iter_mut().for_each(|v| absent_is_none(v, item))
        }
        (value, Shape::Option(inner)) if !value.is_null() => absent_is_none(value, inner),
        _ => {}
    }
}
