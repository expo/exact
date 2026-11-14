//! What the sources' syntax refused while the rest of them was still
//! checked (`contract_syntax::Partial`): one run reports a refused line and
//! what the later passes find in the declarations around it.

use crate::CompileError;
use contract_syntax::Hidden;

/// The syntax refusals of every source the loader read, and the names the
/// refused declarations and members declared.
#[derive(Debug, Default)]
pub(crate) struct Refused {
    pub(crate) errors: Vec<CompileError>,
    /// With the source each was refused in.
    pub(crate) hidden: Vec<(u32, Hidden)>,
    /// A source whose declarations could not all be told apart: nothing after
    /// the syntax is checked.
    pub(crate) whole: bool,
}

impl Refused {
    /// `result`, after the syntax refusals when there are any: then a later
    /// pass's refusal is kept unless a refused declaration's absence caused
    /// it, or it is `type-cannot-infer`, since what completed a type may be in what was
    /// refused (an action that wrote `selected = some(1)`).
    pub(crate) fn join<T>(
        &self,
        result: Result<T, Vec<CompileError>>,
    ) -> Result<T, Vec<CompileError>> {
        if self.errors.is_empty() {
            return result;
        }
        let mut all = self.errors.clone();
        if !self.whole {
            let later = result.err().unwrap_or_default();
            all.extend(
                later
                    .into_iter()
                    .filter(|e| !self.hides(e) && e.id != "type-cannot-infer"),
            );
        }
        Err(all)
    }

    /// Whether `e` reports what a refused declaration would have answered.
    pub(crate) fn hides(&self, e: &CompileError) -> bool {
        self.hidden.iter().any(|(source, h)| caused(e, *source, h))
    }
}

/// Whether `e` is a consequence of `h`'s declaration being refused: a
/// diagnostic only that declaration's kind answers, about its name, within
/// the component a member belongs to. An action passed down as an `action`
/// prop is checked where the child calls it, so what says the name is no
/// action is not held to the component.
fn caused(e: &CompileError, source: u32, h: &Hidden) -> bool {
    let passed = matches!(
        e.id.as_str(),
        "syntax-call-target" | "analyze-unknown-action" | "type-unknown-command"
    );
    if let Some((first, last)) = h.lines.filter(|_| !passed) {
        if e.span.source_id != source || !(first..=last).contains(&e.span.line) {
            return false;
        }
    }
    let (name, message) = (h.name.as_str(), &e.message);
    let member = h.component.is_some();
    let kind = |kinds: &[&str]| kinds.contains(&h.kind.as_str());
    // A `use` line's names may be of any kind.
    let declared = |kinds: &[&str]| !member && (h.kind == "use" || kind(kinds));
    // What a call of the name reaches without the declaration: the `path()`
    // intrinsic, a roster function or no function at all.
    let callable = declared(&["fn", "source"]) || member && kind(&["action"]);
    match e.id.as_str() {
        "contract-use-unknown" => {
            !member
                && message
                    .split_once("` declares no component, shape, style, function, keyframes, or timeline ")
                    .is_some_and(|(_, subject)| subject.starts_with(&format!("`{name}`;")))
        }
        "type-unknown-name" => {
            (member || declared(&["fn", "source", "shape", "routes"]))
                && message.starts_with(&format!("unknown name `{name}`"))
        }
        "type-unknown" => {
            declared(&["shape"]) && message.starts_with(&format!("unknown type `{name}`"))
        }
        "syntax-unknown-component" | "type-unknown-component" | "analyze-unknown-component" => {
            declared(&["component"]) && message.starts_with(&format!("unknown component `{name}`"))
        }
        "type-unknown-function" => {
            (callable || declared(&["shape"]))
                && message.starts_with(&format!("`{name}` is not in the stdlib roster"))
                || callable
                    && (message == &format!("declare `routes` to use `{name}`")
                        || name == "empty"
                            && message
                                == "write `[]` for an empty list; `empty()` is only a resource's `else`")
        }
        "lower-unknown-class" => {
            declared(&["style"]) && message == &format!("`class={name}`: no `style {name}` in this file")
        }
        "lower-animation-name" => {
            declared(&["keyframes"])
                && message == &format!("`animation` names `{name}`, and no `keyframes {name}` is declared")
        }
        "syntax-call-target" | "analyze-unknown-action" | "type-unknown-command" => {
            (callable || member) && message.starts_with(&format!("`{name}` is "))
        }
        "type-assign-not-state" => {
            member && kind(&["state", "mutation"]) && message.starts_with(&format!("`{name}` is "))
        }
        "type-send-not-mutation" => {
            member && kind(&["mutation"]) && message.starts_with(&format!("`{name}` is "))
        }
        "type-refresh-not-resource"
        | "type-refreshes-not-resource"
        | "type-current-argument" => {
            member && kind(&["resource"]) && message.starts_with(&format!("`{name}` is "))
        }
        "type-pending-argument" | "type-failed-argument" | "type-failure-argument" => {
            member
                && kind(&["resource", "mutation"])
                && message.starts_with(&format!("`{name}` is "))
        }
        // `path()` is the intrinsic unless a `fn path` is declared; an
        // action never replaces it. Every `route-unknown` is the intrinsic's,
        // some about the route table without naming `path`.
        "route-unknown" => name == "path" && declared(&["fn"]),
        "type-now-renamed" => {
            callable && name == "now" && message == contract_syntax::idioms::NOW_REFUSED
        }
        "type-refused-idiom" => {
            callable
                && (contract_syntax::idioms::refusal(name).is_some_and(|why| message == &why)
                    || name == "replace"
                        && message.starts_with("`replace(nav, location)` is the router's"))
        }
        // The roster function of the same name checked the call instead.
        "type-argument" => {
            name == "path"
                && declared(&["fn"])
                && message.starts_with("`path()` expects a string or number, given `")
                || callable
                    && message.starts_with("argument ")
                    && message.contains(&format!(" of `{name}` expects "))
        }
        "type-arity" => callable && message.starts_with(&format!("`{name}` takes ")),
        _ => false,
    }
}
