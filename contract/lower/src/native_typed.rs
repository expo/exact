//! A package view's typed props and declared events (LLP 1133.021 §9 items
//! 1 and 2).
//!
//! - **A record or list prop** (`markers: MapMarker[]`) reaches the
//!   implementation as JSON text under the prop's name, in the one object of
//!   strings the view's props are. The pair's key is the list `[name,
//!   schema]`, and `NativeProps` writes the value by the schema
//!   (`stdlib::native_props`, `budget.js` `NP`): `0` a string, number or
//!   bool, `{"o": S}` an option, `{"l": S}` a list, `{"r": [[field, S], …]}`
//!   a record, whose fields the runtimes hold by position.
//! - **A declared event** (`{"event": "moveend", "args": ["MapCamera"]}`) is
//!   heard with `moveend=moved`. The implementation sends it as a message
//!   whose text is `{"exact:event": "moveend", "args": [...]}`; the handler
//!   is a `message` handler whose one bound argument is the marker
//!   `exact:event:moveend`, by which the runtimes choose it and decode the
//!   arguments by the action's parameter types.
//!
//! The driver reads each view's declaration from the app's packages and
//! sets it here for the lowering that follows ([`DeclaredViews::set`]); a
//! prop it declares is held to its type (`contract-view-prop`).

use crate::{err, LowerError, Lowerer};
use contract_syntax::{Attr, Expr};
use contract_types::Ty;
use exact_plan::{ActionsId, Code, EventKind};
use serde_json::{json, Value as Json};
use std::cell::RefCell;
use std::collections::BTreeMap;

/// What marks a declared event's handler: its bound argument's prefix.
pub const EVENT_MARKER: &str = "exact:event:";

/// One package view's declaration, as the bake reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclaredView {
    /// The interface whose implementation serves the view.
    pub extension: String,
    /// Each declared prop's type as the interface writes it
    /// (`option<MapMarker[]>`).
    pub props: BTreeMap<String, String>,
    /// Each event beyond the nine every module view sends, with its
    /// arguments' types.
    pub events: BTreeMap<String, Vec<String>>,
}

thread_local! {
    static DECLARED: RefCell<BTreeMap<String, DeclaredView>> = RefCell::default();
}

/// The views the app's packages declare, by tag, for the lowering on this
/// thread until it is dropped.
pub struct DeclaredViews(());

impl DeclaredViews {
    /// Declare `views` until the guard is dropped.
    pub fn set(views: BTreeMap<String, DeclaredView>) -> Self {
        DECLARED.with(|d| *d.borrow_mut() = views);
        DeclaredViews(())
    }
}

impl Drop for DeclaredViews {
    fn drop(&mut self) {
        DECLARED.with(|d| d.borrow_mut().clear());
    }
}

pub(crate) fn declared(tag: &str) -> Option<DeclaredView> {
    DECLARED.with(|d| d.borrow().get(tag).cloned())
}

pub(crate) fn used_extensions(
    nodes: &[contract_syntax::Node],
) -> std::collections::BTreeSet<String> {
    DECLARED.with(|d| {
        let d = d.borrow();
        crate::native::module_tags_in(nodes)
            .iter()
            .filter_map(|(tag, _)| d.get(tag).map(|v| v.extension.clone()))
            .collect()
    })
}

/// Whether the package view `tag` declares a prop named `name`.
pub(crate) fn declares_prop(tag: &str, name: &str) -> bool {
    DECLARED.with(|d| {
        d.borrow()
            .get(tag)
            .is_some_and(|v| v.props.contains_key(name))
    })
}

/// Whether string literal `value` is one of the cases enum declaration `decl` (or its option) names: a literal
/// infers to `string`, which no `enum<…>` admits otherwise.
pub(crate) fn names_case(decl: &str, value: &str) -> bool {
    let decl = decl.trim();
    let decl = (decl
        .strip_prefix("option<")
        .and_then(|d| d.strip_suffix('>')))
    .unwrap_or(decl);
    (decl.strip_prefix("enum<").and_then(|d| d.strip_suffix('>')))
        .is_some_and(|cases| cases.split('|').any(|c| c.trim() == value))
}

/// Whether a value of type `ty` is what declaration `decl` names, as a
/// prop's value given where the package declares the prop. A declaration
/// the check does not read (`handle<track>`) admits anything.
pub(crate) fn fits(decl: &str, ty: &Ty) -> bool {
    agrees(decl, ty, Flow::Given)
}

/// Whether an action parameter of type `ty` can hold every value of
/// declaration `decl`, as a declared event's argument it receives.
pub(crate) fn receives(ty: &Ty, decl: &str) -> bool {
    agrees(decl, ty, Flow::Received)
}

/// Which way a value crosses between Contract and a package's declaration.
#[derive(Clone, Copy)]
enum Flow {
    /// A Contract value of the type, given where the declaration is.
    Given,
    /// A value of the declaration, received where the type is.
    Received,
}

/// The cases `enum<a|b>` names.
fn cases(decl: &str) -> Option<Vec<&str>> {
    let cases = decl.strip_prefix("enum<")?.strip_suffix('>')?;
    Some(cases.split('|').map(str::trim).collect())
}

fn agrees(decl: &str, ty: &Ty, flow: Flow) -> bool {
    let decl = decl.trim();
    if *ty == Ty::Unknown || decl.starts_with("handle<") {
        return true;
    }
    if let Some(inner) = decl
        .strip_prefix("option<")
        .and_then(|d| d.strip_suffix('>'))
    {
        return match ty {
            Ty::Option(t) => agrees(inner, t, flow),
            t => agrees(inner, t, flow),
        };
    }
    let list = (decl.strip_suffix("[]"))
        .or_else(|| decl.strip_prefix("list<").and_then(|d| d.strip_suffix('>')));
    if let Some(inner) = list {
        return matches!(ty, Ty::List(t) if agrees(inner, t, flow));
    }
    match (ty, flow) {
        (Ty::String, _) => decl == "string",
        // Every case the value can take is one the receiving side names.
        (Ty::Enum(values), Flow::Given) => {
            decl == "string"
                || cases(decl)
                    .is_some_and(|cases| values.iter().all(|v| cases.contains(&v.as_str())))
        }
        (Ty::Enum(values), Flow::Received) => {
            cases(decl).is_some_and(|cases| cases.iter().all(|c| values.iter().any(|v| v == c)))
        }
        (Ty::Number, _) => decl == "number",
        (Ty::Bool, _) => decl == "bool",
        (Ty::Record(name), _) => decl == name,
        _ => false,
    }
}

impl Lowerer<'_> {
    /// `ty`'s schema (module doc); `None` for a type no JSON carries.
    pub(crate) fn prop_schema(&self, ty: &Ty, depth: u32) -> Option<Json> {
        if depth > 16 {
            return None;
        }
        Some(match ty {
            Ty::String | Ty::Enum(_) | Ty::Number | Ty::Bool => json!(0),
            Ty::Option(t) => json!({ "o": self.prop_schema(t, depth + 1)? }),
            Ty::List(t) => json!({ "l": self.prop_schema(t, depth + 1)? }),
            Ty::Record(name) => {
                let fields = self.types.shapes.map.get(name)?;
                let mut out = Vec::new();
                for (field, t) in fields {
                    out.push(json!([field, self.prop_schema(t, depth + 1)?]));
                }
                json!({ "r": out })
            }
            _ => return None,
        })
    }

    /// `event=action` on a view that declares `event` with `args`: a
    /// `message` handler marked as the event's (module doc), the action's
    /// parameters a prefix of the event's arguments, each typed as one.
    pub(crate) fn view_event(
        &mut self,
        tag: &str,
        args: &[String],
        a: &Attr,
        handlers: &mut Vec<(EventKind, ActionsId, Vec<Code>)>,
    ) -> Result<(), LowerError> {
        let event = a.name.as_str();
        let Expr::Ident(name, _) = &a.value else {
            return err(
                "lower-handler",
                format!("`{event}=` on `{tag}` names an action of the root: `{event}=moved`"),
                a.span,
            );
        };
        let Some(ai) = self.action_names.get(name.as_str()).copied() else {
            return err(
                "lower-unknown-action",
                format!("`{name}` is not an action of the root"),
                a.span,
            );
        };
        let params = &self.types.components[0].actions[ai];
        if params.len() > args.len() {
            return err(
                "lower-handler-arity",
                format!(
                    "`{name}` takes {} parameters, and `{tag}`'s `{event}` carries {}: {}",
                    params.len(),
                    args.len(),
                    args.join(", ")
                ),
                a.span,
            );
        }
        for (p, d) in params.iter().zip(args) {
            if *p == Ty::Unknown || !receives(p, d) {
                return err(
                    "lower-handler-type",
                    format!("`{name}` takes `{p}` where `{tag}`'s `{event}` carries `{d}`"),
                    a.span,
                );
            }
        }
        let marker = (self.b).constant(&exact_plan::Value::str(&format!("{EVENT_MARKER}{event}")));
        handlers.push((EventKind::Message, self.actions[ai], vec![marker]));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{fits, names_case, receives};
    use contract_types::Ty;

    #[test]
    fn a_declared_type_admits_its_own_values() {
        let marker = Ty::Record("MapMarker".into());
        let markers = Ty::List(Box::new(marker.clone()));
        assert!(fits("option<MapMarker[]>", &markers));
        assert!(fits("MapMarker[]", &markers));
        assert!(fits("option<number>", &Ty::Option(Box::new(Ty::Number))));
        assert!(fits("handle<track>", &Ty::String));
        assert!(!fits("MapMarker[]", &marker));
        assert!(!fits("number", &Ty::String));
        assert!(!fits("Other[]", &markers));
    }

    #[test]
    fn a_string_literal_admits_an_enum_only_by_one_of_its_cases() {
        let kinds = "option<enum<plain|buy|order>>";
        assert!(!fits(kinds, &Ty::String));
        assert!(names_case(kinds, "order"));
        assert!(names_case("enum<plain|buy>", "plain"));
        assert!(!names_case(kinds, "rent"));
        assert!(!names_case("string", "order"));
    }

    /// A prop takes an enum whose every case it declares; an action
    /// parameter takes an event's enum when it holds every case the event
    /// carries. Both hold under `option` and `list`.
    #[test]
    fn an_enum_agrees_with_a_declaration_by_its_cases() {
        let cases = |cs: &[&str]| Ty::Enum(cs.iter().map(|c| c.to_string()).collect());
        let plain_buy = cases(&["plain", "buy"]);
        let rent = cases(&["rent"]);
        let rent_order = cases(&["rent", "order"]);
        let wrap = |t: &Ty| {
            [
                (t.clone(), "enum<rent|order>"),
                (Ty::Option(Box::new(t.clone())), "option<enum<rent|order>>"),
                (Ty::List(Box::new(t.clone())), "enum<rent|order>[]"),
            ]
        };
        for (t, decl) in wrap(&plain_buy) {
            assert!(!fits(decl, &t), "{t} given as {decl}");
            assert!(!receives(&t, decl), "{decl} received as {t}");
        }
        for (t, decl) in wrap(&rent) {
            assert!(fits(decl, &t), "{t} given as {decl}");
            assert!(!receives(&t, decl), "{decl} received as {t}");
        }
        for (t, decl) in wrap(&cases(&["rent", "order", "late"])) {
            assert!(!fits(decl, &t), "{t} given as {decl}");
            assert!(receives(&t, decl), "{decl} received as {t}");
        }
        assert!(fits("enum<order|rent>", &rent_order));
        assert!(receives(&rent_order, "enum<order|rent>"));
        // A string prop takes any enum; an enum parameter takes no string.
        assert!(fits("string", &plain_buy));
        assert!(!receives(&plain_buy, "string"));
    }
}
