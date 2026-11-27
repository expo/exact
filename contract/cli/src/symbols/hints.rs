//! Action-spelling hints on compiler refusals over the authored source tree.

use crate::{CompileError, RelatedLocation};
use contract_syntax::*;

// Expansion lifts child declarations into the root. Only the original tree
// can say which action spellings the author can use at a failing handler.
// Walk it on refusal only; successful compilation does no diagnostic work.
pub(crate) fn authored_action_hint(file: &File, mut error: CompileError) -> CompileError {
    // An ambiguous call (LLP 1089 D1): the declaration it collides with.
    if error.id == contract_syntax::inline::calls::Ambiguous::ID {
        let found = contract_syntax::inline::calls::ambiguous(file);
        if let Some(a) = found.into_iter().find(|a| a.span == error.span) {
            error.related = Box::new([RelatedLocation {
                span: a.declared,
                file: None,
                note: format!("the {} `{}` is declared here", a.what, a.name),
            }]);
        }
        return error;
    }
    if !matches!(
        error.id.as_str(),
        "type-unknown-name" | "type-unknown-function" | "analyze-unknown-action"
    ) {
        return error;
    }
    let refused = error.message.split('`').nth(1).unwrap_or("").to_owned();
    #[derive(Clone, Copy)]
    struct HintContext<'a> {
        refused: &'a str,
        call: bool,
        provider: bool,
    }
    let context = HintContext {
        refused: &refused,
        call: error.id == "type-unknown-function",
        provider: false,
    };
    fn action_type(file: &File, ty: &Option<TypeExpr>) -> bool {
        matches!(ty, Some(TypeExpr::Named(name, _)) if name == "action")
            && !file.shapes.iter().any(|shape| shape.name == "action")
    }
    fn suggestion<'a>(
        file: &File,
        c: &'a Component,
        name: &str,
        shadowed: &[&str],
        timer: bool,
        call: bool,
    ) -> Option<&'a str> {
        if !name.is_ascii() || !(3..=64).contains(&name.len()) {
            return None;
        }
        let names = c.actions.iter().map(|a| a.name.as_str()).chain(
            c.props
                .iter()
                .chain(&c.injects)
                .filter(|p| !timer && action_type(file, &p.ty))
                .map(|p| p.name.as_str()),
        );
        let mut found = None;
        for candidate in names {
            if shadowed.contains(&candidate)
                || (call
                    && (matches!(
                        candidate,
                        "pending" | "failed" | "failure" | "current" | "path"
                    ) || file.fns.iter().any(|f| f.name == candidate)))
                || !contract_syntax::one_spelling_edit(name.as_bytes(), candidate.as_bytes())
            {
                continue;
            }
            if found.is_some_and(|old| old != candidate) {
                return None;
            }
            found = Some(candidate);
        }
        found
    }
    fn view(
        file: &File,
        c: &Component,
        nodes: &[Node],
        span: Span,
        shadowed: &mut Vec<String>,
        context: HintContext<'_>,
    ) -> Option<(String, Option<String>)> {
        for node in nodes {
            let attrs = match node {
                Node::Element { attrs, .. } => Some((attrs, None)),
                Node::Use { name, args, .. } => file
                    .components
                    .iter()
                    .find(|target| target.name == *name)
                    .map(|target| (args, Some(target))),
                _ => None,
            };
            if let Some((attrs, target)) = attrs {
                for attr in attrs {
                    let is_action = match target {
                        Some(target) => target
                            .props
                            .iter()
                            .any(|p| p.name == attr.name && action_type(file, &p.ty)),
                        None => contract_analyze::HANDLERS.contains(&attr.name.as_str()),
                    };
                    if is_action && attr.value.span() == span {
                        if let Expr::Ident(name, _) | Expr::Call(name, _, _) = &attr.value {
                            if name != context.refused
                                || c.props.iter().chain(&c.injects).any(|p| p.name == *name)
                            {
                                continue;
                            }
                            let locals: Vec<_> = shadowed.iter().map(String::as_str).collect();
                            return Some((
                                name.clone(),
                                suggestion(file, c, name, &locals, false, context.call)
                                    .map(str::to_owned),
                            ));
                        }
                    }
                }
            }
            let found = match node {
                Node::Element { children, .. } | Node::Use { children, .. } => {
                    view(file, c, children, span, shadowed, context)
                }
                Node::Each {
                    var, index, body, ..
                } => {
                    let names = 1 + index.is_some() as usize;
                    shadowed.push(var.clone());
                    shadowed.extend(index.clone());
                    let found = view(file, c, body, span, shadowed, context);
                    shadowed.truncate(shadowed.len() - names);
                    found
                }
                Node::Match { some, none, .. } => {
                    shadowed.push(some.0.clone());
                    let found = view(file, c, &some.1, span, shadowed, context);
                    shadowed.pop();
                    found.or_else(|| view(file, c, none, span, shadowed, context))
                }
                Node::When {
                    then, otherwise, ..
                } => view(file, c, then, span, shadowed, context)
                    .or_else(|| view(file, c, otherwise, span, shadowed, context)),
                Node::Children { .. } => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    // A provided value is written in the component's `provide` section,
    // where no view local shadows an action (LLP 1035.005.000 D9).
    fn provided(
        file: &File,
        c: &Component,
        span: Span,
        context: HintContext<'_>,
    ) -> Option<(String, Option<String>)> {
        let b = c.provides.iter().find(|b| b.expr.span() == span)?;
        (context.provider && expression_name(&b.expr) == Some(context.refused)).then(|| {
            (
                context.refused.to_owned(),
                suggestion(file, c, context.refused, &[], false, context.call).map(str::to_owned),
            )
        })
    }
    fn expression_name(expr: &Expr) -> Option<&str> {
        match expr {
            Expr::Ident(name, _) | Expr::Call(name, _, _) => Some(name),
            _ => None,
        }
    }
    fn authored_expr(c: &Component, span: Span) -> Option<&Expr> {
        if let Some(b) = c.provides.iter().find(|b| b.expr.span() == span) {
            return Some(&b.expr);
        }
        let mut found = None;
        Node::walk(&c.view).for_each(|node| {
            if let Node::Element { attrs, .. } | Node::Use { args: attrs, .. } = node {
                if let Some(a) = attrs.iter().find(|a| a.value.span() == span) {
                    found = Some(&a.value);
                }
            }
        });
        found
    }
    /// What the use at `use_span` in `c`'s view is given for `name`: its
    /// argument, or for an inject `c`'s own provided value (a section covers
    /// the whole view, LLP 1035.005.000 D9).
    fn supplied<'a>(
        c: &'a Component,
        use_span: Span,
        name: &str,
        inject: bool,
    ) -> Option<&'a Expr> {
        let mut args = None;
        Node::walk(&c.view).for_each(|node| {
            if let Node::Use {
                args: given, span, ..
            } = node
            {
                if *span == use_span {
                    args = Some(given);
                }
            }
        });
        let args = args?;
        if inject {
            c.provides.iter().find(|b| b.name == name).map(|b| &b.expr)
        } else {
            args.iter().find(|a| a.name == name).map(|a| &a.value)
        }
    }
    fn origin(
        file: &File,
        instances: &[contract_syntax::Instance],
        mut instance: u32,
        mut span: Span,
        refused: &str,
    ) -> Option<(u32, Span)> {
        loop {
            let site = &instances[instance as usize];
            let c = file.components.iter().find(|c| c.name == site.component)?;
            let expr = authored_expr(c, span)?;
            let name = expression_name(expr)?;
            let inject = c.injects.iter().any(|p| p.name == name);
            if name == refused && !inject && !c.props.iter().any(|p| p.name == name) {
                return Some((instance, span));
            }
            if !inject && !c.props.iter().any(|p| p.name == name) {
                return None;
            }
            let mut child = instance;
            loop {
                let site = &instances[child as usize];
                let parent = site.parent?;
                let c = file
                    .components
                    .iter()
                    .find(|c| c.name == instances[parent as usize].component)?;
                if let Some(expr) = supplied(c, site.span, name, inject) {
                    instance = parent;
                    span = expr.span();
                    break;
                }
                if !inject {
                    return None;
                }
                child = parent;
            }
        }
    }
    for c in &file.components {
        let found = if error.id == "analyze-unknown-action" {
            c.tasks
                .iter()
                .find(|task| task.timer.2 == error.span)
                .map(|task| {
                    (
                        task.timer.1.clone(),
                        suggestion(file, c, &task.timer.1, &[], true, false).map(str::to_owned),
                    )
                })
        } else {
            view(file, c, &c.view, error.span, &mut Vec::new(), context)
        };
        if let Some((_name, candidate)) = found {
            // A similarly spelled global function cannot repair an action-valued
            // position. Replace only the old hint, retaining the refusal and span.
            if let Some(at) = error.message.find("; did you mean `") {
                error.message.truncate(at);
            }
            if let Some(candidate) = candidate {
                error
                    .message
                    .push_str(&format!("; did you mean `{candidate}`?"));
            }
            return error;
        }
    }
    if error.id != "analyze-unknown-action" {
        if let Ok(expanded) = contract_syntax::expand_mapped(file) {
            let mut sites = Vec::new();
            Node::walk(&expanded.root.view).for_each(|node| {
                if let Node::Element {
                    attrs, instance, ..
                } = node
                {
                    if attrs.iter().any(|a| {
                        contract_analyze::HANDLERS.contains(&a.name.as_str())
                            && a.value.span() == error.span
                            && expression_name(&a.value) == Some(refused.as_str())
                    }) {
                        sites.push(*instance);
                    }
                }
            });
            if !sites.is_empty() {
                if let Some(at) = error.message.find("; did you mean `") {
                    error.message.truncate(at);
                }
            }
            let mut resolved = Vec::new();
            for instance in sites {
                let Some((owner, span)) =
                    origin(file, &expanded.instances, instance, error.span, &refused)
                else {
                    return error;
                };
                let c = file
                    .components
                    .iter()
                    .find(|c| c.name == expanded.instances[owner as usize].component)
                    .unwrap();
                let context = HintContext {
                    provider: true,
                    ..context
                };
                let Some((_, Some(candidate))) = provided(file, c, span, context)
                    .or_else(|| view(file, c, &c.view, span, &mut Vec::new(), context))
                else {
                    return error;
                };
                resolved.push((span, candidate));
            }
            if let Some((_, candidate)) = resolved.first() {
                if resolved.iter().all(|(_, name)| name == candidate) {
                    if let Some(at) = error.message.find("; did you mean `") {
                        error.message.truncate(at);
                    }
                    error
                        .message
                        .push_str(&format!("; did you mean `{candidate}`?"));
                    let mut related = error.related.into_vec();
                    for (span, _) in resolved {
                        if !related.iter().any(|r| r.span == span) {
                            related.push(RelatedLocation {
                                span,
                                file: None,
                                note: format!("the unknown action `{refused}` is supplied here"),
                            });
                        }
                    }
                    error.related = related.into_boxed_slice();
                }
            }
        }
    }
    error
}
