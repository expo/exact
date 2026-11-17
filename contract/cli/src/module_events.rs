//! A native module's events (LLP 1133.004 D6): the app hears one with an
//! attribute on its root component's outermost node, as `navigate=` is
//! written on a navigation root: `column notificationclick=opened`. The
//! attribute names an action of the root, which takes a prefix of the
//! event's arguments; an untyped parameter takes the event's type. The
//! attribute leaves the node here, and the binding stays on the event's
//! declaration for lowering to put in the plan.

use crate::modules::{error, same, written};
use crate::CompileError;
use contract_syntax::scope::{Kind, Scope};
use contract_syntax::{Attr, Expr, File, ModuleKind, Node};
use std::collections::HashMap;

/// Resolve event attributes through their file's scope (LLP 1091 D1/D2),
/// bind each to its root action, and refuse the attribute on any other element.
pub(crate) fn bind(file: &mut File, scopes: &[Scope], errors: &mut Vec<CompileError>) {
    let events: HashMap<String, usize> = (file.modules.iter().enumerate())
        .filter(|(_, m)| m.kind == ModuleKind::Event)
        .map(|(i, m)| (m.name.clone(), i))
        .collect();
    if events.is_empty() {
        return;
    }
    let mut found: Vec<(usize, Attr)> = Vec::new();
    for (ci, c) in file.components.iter_mut().enumerate() {
        for node in c.view.iter_mut() {
            let outermost = ci == 0 && matches!(node, Node::Element { .. });
            take(node, &events, scopes, outermost, &mut found, errors);
        }
    }
    for (mi, attr) in found {
        let name = &attr.name;
        let Expr::Ident(action, _) = &attr.value else {
            errors.push(error(
                "contract-event-handler",
                format!("`{name}=` names an action of the root, `{name}=opened`"),
                attr.span,
            ));
            continue;
        };
        let m = &file.modules[mi];
        if m.handler.is_some() {
            errors.push(error(
                "contract-event-root",
                format!("`{name}` is heard once, on the root component's outermost node"),
                attr.span,
            ));
            continue;
        }
        let args = m.args.clone();
        let Some(a) = (file.components.first_mut())
            .and_then(|root| root.actions.iter_mut().find(|a| a.name == *action))
        else {
            errors.push(error(
                "contract-event-handler",
                format!("`{name}={action}`: the root component has no action `{action}`"),
                attr.span,
            ));
            continue;
        };
        if a.params.len() > args.len() {
            errors.push(error(
                "contract-event-handler",
                format!(
                    "`{action}` takes {} parameters, and `{name}` carries {}",
                    a.params.len(),
                    args.len()
                ),
                a.span,
            ));
            continue;
        }
        let mut ok = true;
        for (p, ty) in a.params.iter_mut().zip(&args) {
            match &p.ty {
                None => p.ty = Some(ty.clone()),
                Some(written_ty) if same(written_ty, ty) => {}
                Some(_) => {
                    ok = false;
                    errors.push(error(
                        "contract-event-handler",
                        format!(
                            "`{}` of `{action}` is `{}` here: `{name}` carries `{}`",
                            p.name,
                            written(p.ty.as_ref().expect("written")),
                            written(ty)
                        ),
                        p.span,
                    ));
                }
            }
        }
        if ok {
            file.modules[mi].handler = Some(action.clone());
        }
    }
}

/// Take the event attributes off `node` and the nodes under it: kept from
/// the root's outermost node, refused from every other.
fn take(
    node: &mut Node,
    events: &HashMap<String, usize>,
    scopes: &[Scope],
    outermost: bool,
    found: &mut Vec<(usize, Attr)>,
    errors: &mut Vec<CompileError>,
) {
    if let Node::Element { attrs, .. } = node {
        let mut i = 0;
        while i < attrs.len() {
            let attr = &attrs[i];
            let event = scopes[attr.span.source_id as usize]
                .names
                .get(&(Kind::Call, attr.name.clone()))
                .and_then(|name| events.get(name));
            let Some(&event) = event else {
                i += 1;
                continue;
            };
            let attr = attrs.remove(i);
            if outermost {
                found.push((event, attr));
            } else {
                errors.push(error(
                    "contract-event-root",
                    format!(
                        "`{}` is heard on the root component's outermost node, where every app has one",
                        attr.name
                    ),
                    attr.span,
                ));
            }
        }
    }
    for list in node.child_lists_mut() {
        for child in list.iter_mut() {
            take(child, events, scopes, false, found, errors);
        }
    }
}
